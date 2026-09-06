//! The attached Harness as the Completer: an ACP client over stdio.
//!
//! The sibling of `model.rs`. Where that file posts a Character Prompt to a
//! chat-completions host, this one spawns the Harness in ACP mode and makes
//! every wake one `session/prompt` in one session (ADR-0008, ADR-0010). The
//! frame loop never sees any of it: `complete` runs on a `Slots` worker.
//!
//! Hand-rolled newline JSON-RPC over `std::process::Child`. ADR-0017 says why
//! neither `acp-cli` nor the protocol crates are here: both bring an async
//! runtime, and the surface we speak is eight messages.
//!
//! Authentication is the Harness's own. Nothing here sets a provider key,
//! reads a credential, or calls `authenticate`; `auth_required` becomes a
//! command the user runs in their own terminal (ADR-0010's eight rules).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use ai_buddy_core::director::Completer;
use serde::Serialize;
use serde_json::{json, Value};

use crate::action_log;

/// `pub(crate)` like `model::API_KEY`: the settings window names the variable
/// that owns a row.
pub(crate) const VAR: &str = "AI_BUDDY_HARNESS";
/// Where the stdio MCP server binary is, when it is not beside the app.
pub(crate) const MCP_BIN: &str = "AI_BUDDY_MCP_BIN";

/// The one file the session survives a restart in.
const SESSION_FILE: &str = "harness-session.json";

/// How long a not-yet-authenticated Harness is left alone before `session/new`
/// is tried again. Long enough not to hammer it, short enough that a user who
/// runs the login command sees the buddy pick it up without a restart.
const AUTH_RETRY: Duration = Duration::from_secs(60);

/// Respawn backoff after a spawn that failed: doubles from the first up to the
/// cap, so a missing binary costs one attempt every five minutes, not a loop.
const BACKOFF_FIRST: Duration = Duration::from_secs(5);
const BACKOFF_CAP: Duration = Duration::from_secs(5 * 60);

/// After `session/cancel`, how long to wait for the `cancelled` reply before
/// giving the turn lock back regardless.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// JSON-RPC codes ACP gives a meaning.
const AUTH_REQUIRED: i64 = -32000;
const METHOD_NOT_FOUND: i64 = -32601;

/// Which Harness, and the command line that starts it in ACP mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub name: String,
    pub argv: Vec<String>,
}

/// The launch table. `None` means the HTTP Completer path, unchanged.
///
/// Claude Code goes through Zed's adapter because it has no first-party ACP
/// mode; `hermes acp` and `opencode acp` are first-party. Anything else is a
/// command line of the user's own. Pi, Grok Build, Codex, Gemini and Copilot
/// are deferred (ADR-0017).
pub fn launch(value: Option<&str>) -> Option<Launch> {
    let value = value?.trim();
    let argv: Vec<String> = match value {
        "" => return None,
        "claude" => ["npx", "-y", "@agentclientprotocol/claude-agent-acp"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        "hermes" => vec!["hermes".into(), "acp".into()],
        "opencode" => vec!["opencode".into(), "acp".into()],
        custom => custom.split_whitespace().map(str::to_string).collect(),
    };
    let name = match value {
        "claude" | "hermes" | "opencode" => value.to_string(),
        _ => argv[0].clone(),
    };
    Some(Launch { name, argv })
}

pub fn from_env() -> Option<Launch> {
    launch(std::env::var(VAR).ok().as_deref())
}

impl Launch {
    /// The child, inheriting our environment untouched. ADR-0010 rules 4 and
    /// 5: no provider key, no `CLAUDE_CONFIG_DIR`, no `--bare`. A test pins it.
    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command
            .args(&self.argv[1..])
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        command
    }

    fn line(&self) -> String {
        self.argv.join(" ")
    }
}

/// A forwarded `session/request_permission`, as the Chat surface draws it.
#[derive(Clone, Debug, Serialize)]
pub struct PermissionAsk {
    /// The Harness's own request id, handed back with the answer.
    pub request: Value,
    pub title: String,
    pub kind: Option<String>,
    pub options: Vec<PermissionOption>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
}

/// What Settings and the Chat surface can say about the attachment.
#[derive(Clone, Debug, Default, Serialize)]
pub struct HarnessInspect {
    pub name: String,
    pub command: String,
    pub agent: Option<String>,
    pub session_id: Option<String>,
    /// Attached but not authenticated: the one command that fixes it.
    pub login: Option<String>,
    /// Whether `initialize` offered HTTP MCP. #166 branches on it.
    pub mcp_http: bool,
    pub alive: bool,
}

type Forward = Box<dyn Fn(PermissionAsk) + Send + Sync>;

/// One ACP session for the app's lifetime, shared by every Instance.
pub struct Session {
    launch: Launch,
    dir: PathBuf,
    forward: Arc<Forward>,
    timeout: Duration,
    auth_retry: Duration,
    /// One prompt in flight. `try_lock` failing is "harness busy", never a
    /// queue (ADR-0008, ADR-0016).
    turn: Mutex<()>,
    /// Bookkeeping, and the blocking `initialize`/`session/*` hop under it, so
    /// two wakes cannot open two sessions.
    state: Mutex<State>,
    /// Separate from `state` so `shutdown` never waits on an attach in flight.
    link: Mutex<Option<Arc<Link>>>,
    inspect: Mutex<HarnessInspect>,
}

#[derive(Default)]
struct State {
    session_id: Option<String>,
    load_session: bool,
    auth_methods: Vec<Value>,
    login: Option<String>,
    auth_tried: Option<Instant>,
    spawn_failures: u32,
    spawn_wait_until: Option<Instant>,
}

impl Session {
    pub fn new(launch: Launch, dir: PathBuf, forward: Forward) -> Self {
        let inspect = HarnessInspect {
            name: launch.name.clone(),
            command: launch.line(),
            ..Default::default()
        };
        Self {
            launch,
            dir,
            forward: Arc::new(forward),
            timeout: crate::dev_flags::director_timeout_secs()
                .map_or(crate::model::TIMEOUT, Duration::from_secs),
            auth_retry: AUTH_RETRY,
            turn: Mutex::new(()),
            state: Mutex::new(State::default()),
            link: Mutex::new(None),
            inspect: Mutex::new(inspect),
        }
    }

    #[cfg(test)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[cfg(test)]
    pub fn with_auth_retry(mut self, retry: Duration) -> Self {
        self.auth_retry = retry;
        self
    }

    pub fn inspect(&self) -> HarnessInspect {
        self.inspect
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn update_inspect(&self, apply: impl FnOnce(&mut HarnessInspect)) {
        if let Ok(mut inspect) = self.inspect.lock() {
            apply(&mut inspect);
        }
    }

    /// `initialize` and `session/*` get at least ten seconds whatever the turn
    /// timeout is: `npx` starting Zed's adapter cold is slower than any reply.
    fn attach_timeout(&self) -> Duration {
        self.timeout.max(Duration::from_secs(10))
    }

    fn log(&self, event: &str, fields: Value) {
        action_log::append(&self.dir, event, fields);
    }

    /// Where the Harness should be by the time the first wake arrives. Spawned
    /// so startup does not wait on `npx`; the outcome is one stderr line.
    pub fn spawn_preflight(self: &Arc<Self>) {
        let session = Arc::clone(self);
        thread::spawn(move || match session.attach() {
            Ok((_, id)) => eprintln!("harness: {} attached, session {id}", session.launch.name),
            Err(why) => eprintln!("harness: {why}; using StaticDirector until it answers"),
        });
    }

    /// One turn. The whole of `Completer::complete`.
    pub fn complete(&self, prompt: &str) -> Result<String, String> {
        let Ok(_turn) = self.turn.try_lock() else {
            return Err("harness busy".to_string());
        };
        let (link, session_id) = self.attach()?;
        link.begin_turn();
        self.log(
            "prompt",
            json!({"session_id": session_id, "chars": prompt.len()}),
        );
        let receiver = link
            .request(
                "session/prompt",
                json!({
                    "sessionId": session_id,
                    "prompt": [{"type": "text", "text": prompt}],
                }),
            )
            .map_err(|_| self.lost(&link))?;
        let reply = match receiver.recv_timeout(self.timeout) {
            Ok(reply) => reply,
            Err(RecvTimeoutError::Disconnected) => return Err(self.lost(&link)),
            Err(RecvTimeoutError::Timeout) => {
                link.notify("session/cancel", json!({"sessionId": session_id}));
                link.cancel_asks();
                let _ = receiver.recv_timeout(CANCEL_GRACE);
                self.log("timeout", json!({"session_id": session_id}));
                return Err(format!(
                    "harness turn exceeded {}s; cancelled",
                    self.timeout.as_secs()
                ));
            }
        };
        let text = link.take_text();
        match reply {
            Ok(result) => {
                let stop = turn_finished(&result);
                self.log("turn", json!({"stop": stop.as_ref().err(), "text": text}));
                stop?;
                if let Ok(mut state) = self.state.lock() {
                    state.spawn_failures = 0;
                }
                Ok(text)
            }
            Err(error) => {
                self.log("turn", json!({"error": error}));
                Err(format!("harness: {}", error_text(&error)))
            }
        }
    }

    /// The link died under a turn. Say so, and let the next wake respawn.
    fn lost(&self, link: &Link) -> String {
        link.dead.store(true, Ordering::SeqCst);
        self.update_inspect(|inspect| inspect.alive = false);
        "harness exited".to_string()
    }

    /// The user's answer to a forwarded permission request. Never chosen here.
    pub fn answer_permission(&self, request: &Value, option: &str) {
        let Some(link) = self.current_link() else {
            return;
        };
        self.log(
            "permission_answer",
            json!({"request": request, "option": option}),
        );
        link.answer_ask(request, option);
    }

    /// On exit: cancel what is in flight, close stdin, give the Harness two
    /// seconds to leave, then kill it.
    pub fn shutdown(&self) {
        let Some(link) = self.link.lock().ok().and_then(|mut slot| slot.take()) else {
            return;
        };
        if let Some(session_id) = self
            .state
            .try_lock()
            .ok()
            .and_then(|s| s.session_id.clone())
        {
            link.notify("session/cancel", json!({"sessionId": session_id}));
        }
        link.cancel_asks();
        link.close();
    }

    fn current_link(&self) -> Option<Arc<Link>> {
        self.link
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .filter(|link| !link.dead.load(Ordering::SeqCst))
    }

    /// A live child and an open session, spawning and negotiating as needed.
    fn attach(&self) -> Result<(Arc<Link>, String), String> {
        let mut state = self.state.lock().map_err(|_| "harness state poisoned")?;
        let link = match self.current_link() {
            Some(link) => link,
            None => {
                state.session_id = None;
                if let Some(until) = state.spawn_wait_until {
                    if Instant::now() < until {
                        return Err(format!(
                            "harness {} not running; retrying in {}s",
                            self.launch.line(),
                            (until - Instant::now()).as_secs()
                        ));
                    }
                }
                match self.spawn_and_initialize(&mut state) {
                    Ok(link) => link,
                    Err(why) => {
                        state.spawn_failures += 1;
                        state.spawn_wait_until =
                            Some(Instant::now() + backoff(state.spawn_failures));
                        return Err(why);
                    }
                }
            }
        };
        if let Some(id) = &state.session_id {
            return Ok((link, id.clone()));
        }
        if let (Some(command), Some(tried)) = (&state.login, state.auth_tried) {
            if tried.elapsed() < self.auth_retry {
                return Err(format!("harness not authenticated: run `{command}`"));
            }
        }
        let id = self.open_session(&link, &mut state)?;
        Ok((link, id))
    }

    fn spawn_and_initialize(&self, state: &mut State) -> Result<Arc<Link>, String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|why| format!("{}: {why}", self.dir.display()))?;
        let child = self
            .launch
            .command(&self.dir)
            .spawn()
            .map_err(|why| format!("could not start `{}`: {why}", self.launch.line()))?;
        let link = Link::start(child, Arc::clone(&self.forward), self.dir.clone());
        let result = link
            .request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    // No fs, no terminal: the Harness works on its own files,
                    // not ours, and anything it asks anyway gets method-not-found.
                    "clientCapabilities": {},
                    "clientInfo": {"name": "ai-buddy", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .map_err(|_| "harness exited before initialize".to_string())?
            .recv_timeout(self.attach_timeout())
            .map_err(|_| {
                link.close();
                "harness did not answer initialize".to_string()
            })?
            .map_err(|error| {
                link.close();
                format!("initialize: {}", error_text(&error))
            })?;
        state.load_session = result
            .pointer("/agentCapabilities/loadSession")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        state.auth_methods = result
            .get("authMethods")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let agent = result
            .pointer("/agentInfo/name")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mcp_http = result
            .pointer("/agentCapabilities/mcpCapabilities/http")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.update_inspect(|inspect| {
            inspect.agent = agent;
            inspect.mcp_http = mcp_http;
            inspect.alive = true;
        });
        if let Ok(mut slot) = self.link.lock() {
            *slot = Some(Arc::clone(&link));
        }
        Ok(link)
    }

    /// `session/load` when the Harness can and the file names this Harness,
    /// else `session/new`. Either way the file ends up naming what is open.
    fn open_session(&self, link: &Arc<Link>, state: &mut State) -> Result<String, String> {
        let (servers, mcp_note) = mcp_servers();
        let cwd = self.dir.to_string_lossy().to_string();
        let saved = self.saved_session();
        let mut id = None;
        if state.load_session {
            if let Some(saved) = saved {
                let loaded = link
                    .request(
                        "session/load",
                        json!({"sessionId": saved, "cwd": cwd, "mcpServers": servers}),
                    )
                    .ok()
                    .and_then(|rx| rx.recv_timeout(self.attach_timeout()).ok());
                if matches!(loaded, Some(Ok(_))) {
                    id = Some(saved);
                }
            }
        }
        let id = match id {
            Some(id) => id,
            None => {
                let reply = link
                    .request("session/new", json!({"cwd": cwd, "mcpServers": servers}))
                    .map_err(|_| self.lost(link))?
                    .recv_timeout(self.attach_timeout())
                    .map_err(|_| self.lost(link))?;
                match reply {
                    Ok(result) => result
                        .get("sessionId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .ok_or_else(|| "session/new returned no sessionId".to_string())?,
                    Err(error) if error_code(&error) == Some(AUTH_REQUIRED) => {
                        let command = login_command(&self.launch.name, &state.auth_methods);
                        state.login = Some(command.clone());
                        state.auth_tried = Some(Instant::now());
                        self.update_inspect(|inspect| inspect.login = Some(command.clone()));
                        return Err(format!("harness not authenticated: run `{command}`"));
                    }
                    Err(error) => return Err(format!("session/new: {}", error_text(&error))),
                }
            }
        };
        state.session_id = Some(id.clone());
        state.login = None;
        self.update_inspect(|inspect| {
            inspect.login = None;
            inspect.session_id = Some(id.clone());
        });
        self.save_session(&id);
        self.log(
            "attach",
            json!({"harness": self.launch.name, "session_id": id, "mcp": mcp_note}),
        );
        Ok(id)
    }

    fn saved_session(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.dir.join(SESSION_FILE)).ok()?;
        let saved: Value = serde_json::from_str(&text).ok()?;
        (saved.get("harness")?.as_str()? == self.launch.name)
            .then(|| saved.get("session_id")?.as_str().map(str::to_string))
            .flatten()
    }

    fn save_session(&self, id: &str) {
        let agent = self.inspect().agent;
        let record = json!({"session_id": id, "harness": self.launch.name, "agent": agent});
        let _ = std::fs::write(self.dir.join(SESSION_FILE), format!("{record}\n"));
    }
}

impl Completer for Session {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        if crate::model::tracing() {
            eprintln!("harness: prompt to {}", self.launch.name);
        }
        let reply = Session::complete(self, prompt);
        if crate::model::tracing() {
            match &reply {
                Ok(text) => eprintln!("harness: reply {text}"),
                Err(why) => eprintln!("harness: {why}"),
            }
        }
        reply
    }
}

/// Where "the turn finished" is read. ACP v2 moves this to an idle
/// `state_update`; keep it here and nowhere else.
fn turn_finished(result: &Value) -> Result<(), String> {
    match result.get("stopReason").and_then(Value::as_str) {
        Some("end_turn") => Ok(()),
        Some(other) => Err(format!("harness stopped: {other}")),
        None => Err("harness reply named no stopReason".to_string()),
    }
}

fn backoff(failures: u32) -> Duration {
    BACKOFF_FIRST
        .saturating_mul(1u32 << failures.saturating_sub(1).min(16))
        .min(BACKOFF_CAP)
}

/// The command that logs the user in, in the Harness's own words where it
/// has any. Claude Code's adapter reports the method but not the command.
fn login_command(name: &str, methods: &[Value]) -> String {
    if name == "claude" {
        return "claude /login".to_string();
    }
    methods
        .first()
        .and_then(|method| {
            method
                .get("description")
                .or_else(|| method.get("name"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("{name} (run it once in a terminal and sign in)"))
}

/// The stdio MCP server to hand the session, when the binary can be found.
///
/// Beside the app, or wherever `AI_BUDDY_MCP_BIN` points. No loopback HTTP
/// server exists yet (#166), so a missing binary means no tools this session.
fn mcp_servers() -> (Vec<Value>, String) {
    let candidate = std::env::var_os(MCP_BIN).map(PathBuf::from).or_else(|| {
        let beside = std::env::current_exe().ok()?.parent()?.join("ai-buddy-mcp");
        Some(if cfg!(windows) {
            beside.with_extension("exe")
        } else {
            beside
        })
    });
    match candidate.filter(|path| path.is_file()) {
        Some(path) => {
            let path = path.to_string_lossy().to_string();
            (
                vec![json!({"name": "ai-buddy", "command": path, "args": [], "env": []})],
                path,
            )
        }
        None => (Vec::new(), "none".to_string()),
    }
}

fn error_code(error: &Value) -> Option<i64> {
    error.get("code").and_then(Value::as_i64)
}

fn error_text(error: &Value) -> String {
    match error.get("message").and_then(Value::as_str) {
        Some(message) => message.to_string(),
        None => error.to_string(),
    }
}

type Reply = Result<Value, Value>;

/// One spawned child and the reader thread on its stdout.
struct Link {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Sender<Reply>>>,
    /// The in-flight turn's `agent_message_chunk`s, in order.
    text: Mutex<String>,
    /// Permission requests forwarded and not yet answered, by request id.
    asks: Mutex<Vec<Value>>,
    dead: AtomicBool,
}

impl Link {
    fn start(mut child: Child, forward: Arc<Forward>, dir: PathBuf) -> Arc<Self> {
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let link = Arc::new(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            text: Mutex::new(String::new()),
            asks: Mutex::new(Vec::new()),
            dead: AtomicBool::new(false),
        });
        if let Some(stdout) = stdout {
            let reader = Arc::clone(&link);
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    match serde_json::from_str::<Value>(&line) {
                        Ok(message) if message.is_object() => {
                            reader.dispatch(message, &forward, &dir)
                        }
                        _ => {
                            if crate::model::tracing() {
                                eprintln!("harness: skipped non-JSON line: {line}");
                            }
                        }
                    }
                }
                reader.dead.store(true, Ordering::SeqCst);
                // Dropping the senders wakes every waiter with Disconnected.
                if let Ok(mut pending) = reader.pending.lock() {
                    pending.clear();
                }
            });
        }
        link
    }

    fn dispatch(&self, message: Value, forward: &Forward, dir: &Path) {
        match message.get("method").and_then(Value::as_str) {
            Some("session/update") => self.update(message.pointer("/params/update"), dir),
            Some("session/request_permission") => {
                let Some(id) = message.get("id").cloned() else {
                    return;
                };
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                let ask = PermissionAsk {
                    request: id.clone(),
                    title: params
                        .pointer("/toolCall/title")
                        .and_then(Value::as_str)
                        .unwrap_or("(untitled)")
                        .to_string(),
                    kind: params
                        .pointer("/toolCall/kind")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    options: params
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|options| {
                            options
                                .iter()
                                .filter_map(|option| {
                                    Some(PermissionOption {
                                        id: option.get("optionId")?.as_str()?.to_string(),
                                        name: option
                                            .get("name")
                                            .and_then(Value::as_str)
                                            .unwrap_or("")
                                            .to_string(),
                                        kind: option
                                            .get("kind")
                                            .and_then(Value::as_str)
                                            .map(str::to_string),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                action_log::append(
                    dir,
                    "permission_request",
                    json!({"request": id, "title": ask.title, "kind": ask.kind}),
                );
                if let Ok(mut asks) = self.asks.lock() {
                    asks.push(id);
                }
                forward(ask);
            }
            // fs/*, terminal/*, and anything else we declared no capability
            // for. A notification we do not know is simply dropped.
            Some(method) => {
                if let Some(id) = message.get("id").cloned() {
                    self.write(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": METHOD_NOT_FOUND, "message": format!("{method} not supported")},
                    }));
                }
            }
            None => {
                let Some(id) = message.get("id").and_then(Value::as_u64) else {
                    return;
                };
                let reply = match (message.get("result"), message.get("error")) {
                    (Some(result), _) => Ok(result.clone()),
                    (None, Some(error)) => Err(error.clone()),
                    (None, None) => return,
                };
                // An id nobody is waiting on is a reply to a superseded or
                // timed-out request: dropped.
                if let Some(sender) = self.pending.lock().ok().and_then(|mut p| p.remove(&id)) {
                    let _ = sender.send(reply);
                }
            }
        }
    }

    fn update(&self, update: Option<&Value>, dir: &Path) {
        let Some(update) = update else { return };
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("");
        match kind {
            "agent_message_chunk" => {
                if let Some(text) = update.pointer("/content/text").and_then(Value::as_str) {
                    if let Ok(mut buffer) = self.text.lock() {
                        buffer.push_str(text);
                    }
                }
            }
            "tool_call" | "tool_call_update" => action_log::append(
                dir,
                kind,
                json!({
                    "id": update.get("toolCallId"),
                    "title": update.get("title"),
                    "kind": update.get("kind"),
                    "status": update.get("status"),
                }),
            ),
            "plan" => action_log::append(dir, kind, json!({"entries": update.get("entries")})),
            "usage_update" => action_log::append(
                dir,
                kind,
                json!({"used": update.get("used"), "size": update.get("size"), "cost": update.get("cost")}),
            ),
            _ => {}
        }
    }

    fn begin_turn(&self) {
        if let Ok(mut text) = self.text.lock() {
            text.clear();
        }
    }

    fn take_text(&self) -> String {
        self.text
            .lock()
            .map(|mut text| std::mem::take(&mut *text))
            .unwrap_or_default()
    }

    /// Send a request; the receiver yields the reply, or disconnects when the
    /// child is gone. `Err` means the write itself failed.
    fn request(&self, method: &str, params: Value) -> Result<Receiver<Reply>, ()> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(id, tx);
        }
        if self.dead.load(Ordering::SeqCst) {
            return Err(());
        }
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .then_some(rx)
            .ok_or(())
    }

    fn notify(&self, method: &str, params: Value) {
        self.write(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn write(&self, message: Value) -> bool {
        let Ok(mut stdin) = self.stdin.lock() else {
            return false;
        };
        let Some(pipe) = stdin.as_mut() else {
            return false;
        };
        let written = writeln!(pipe, "{message}").and_then(|()| pipe.flush());
        if written.is_err() {
            self.dead.store(true, Ordering::SeqCst);
        }
        written.is_ok()
    }

    fn answer_ask(&self, request: &Value, option: &str) {
        let Ok(mut asks) = self.asks.lock() else {
            return;
        };
        let Some(at) = asks.iter().position(|id| id == request) else {
            return;
        };
        asks.remove(at);
        self.write(json!({
            "jsonrpc": "2.0",
            "id": request,
            "result": {"outcome": {"outcome": "selected", "optionId": option}},
        }));
    }

    /// The protocol-mandated reply for a question nobody will answer now:
    /// the turn is cancelled or the app is leaving. Not an answer.
    fn cancel_asks(&self) {
        let Ok(mut asks) = self.asks.lock() else {
            return;
        };
        for id in asks.drain(..) {
            self.write(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"outcome": {"outcome": "cancelled"}},
            }));
        }
    }

    /// Close stdin, wait `CANCEL_GRACE`, kill.
    fn close(&self) {
        self.dead.store(true, Ordering::SeqCst);
        if let Ok(mut stdin) = self.stdin.lock() {
            stdin.take();
        }
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        let until = Instant::now() + CANCEL_GRACE;
        while Instant::now() < until {
            if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

static ATTACHED: OnceLock<Option<Arc<Session>>> = OnceLock::new();

/// Read `AI_BUDDY_HARNESS` once and hold the Session for the app's lifetime.
///
/// A process global rather than a field threaded through `DirectorSettings`:
/// the variable is env-only, the session is one per app (ADR-0008), and a
/// Retarget from Settings rebuilds `DirectorSettings` from scratch, which
/// would drop a field.
pub fn attach(forward: Forward) -> Option<Arc<Session>> {
    ATTACHED
        .get_or_init(|| {
            from_env().map(|launch| {
                Arc::new(Session::new(
                    launch,
                    ai_buddy_core::memory::data_dir(),
                    forward,
                ))
            })
        })
        .clone()
}

pub fn attached() -> Option<Arc<Session>> {
    ATTACHED.get().cloned().flatten()
}

/// What `startup_lines` says about the attachment, if there is one.
pub fn startup_lines() -> Vec<String> {
    let Some(session) = attached() else {
        return Vec::new();
    };
    let (_, mcp) = mcp_servers();
    vec![
        format!(
            "harness: {} via `{}`",
            session.launch.name,
            session.launch.line()
        ),
        match mcp.as_str() {
            "none" => {
                "harness: no ai-buddy-mcp binary found; the session gets no MCP servers".to_string()
            }
            path => format!("harness: MCP server {path}"),
        },
    ]
}

pub fn shutdown() {
    if let Some(session) = attached() {
        session.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fake ACP agent: this test binary re-executed with `script=<name>`
    /// among its filters, speaking newline JSON-RPC on stdio. Returns at once
    /// under a normal `cargo test`, where no script is named.
    #[test]
    fn fake_acp_agent() {
        let args: Vec<String> = std::env::args().collect();
        let Some(script) = args.iter().find_map(|arg| arg.strip_prefix("script=")) else {
            return;
        };
        let count = args
            .iter()
            .find_map(|arg| arg.strip_prefix("count="))
            .map(PathBuf::from);
        fake_main(script, count.as_deref());
        std::process::exit(0);
    }

    fn record(count: Option<&Path>, what: &str) {
        if let Some(path) = count {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            writeln!(file, "{what}").unwrap();
        }
    }

    fn recorded(count: Option<&Path>, what: &str) -> usize {
        count
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map_or(0, |text| text.lines().filter(|line| *line == what).count())
    }

    fn say(value: Value) {
        println!("{value}");
    }

    fn chunk(session: &str, text: &str) {
        say(
            json!({"jsonrpc": "2.0", "method": "session/update", "params": {
                "sessionId": session,
                "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}},
            }}),
        );
    }

    fn stop(id: &Value, reason: &str) {
        say(json!({"jsonrpc": "2.0", "id": id, "result": {"stopReason": reason}}));
    }

    fn fake_main(script: &str, count: Option<&Path>) {
        // libtest writes `test <name> ... ` with no newline before the test
        // runs; end that line so the first reply is a line of its own.
        println!();
        record(count, "spawn");
        let spawns = recorded(count, "spawn");
        let session = "fresh-id";
        let mut pending_prompt: Option<Value> = None;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let id = message.get("id").cloned().unwrap_or(Value::Null);
            match message.get("method").and_then(Value::as_str) {
                Some("initialize") => say(json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": 1,
                    "agentInfo": {"name": "fake-agent"},
                    "agentCapabilities": {"loadSession": script == "load", "mcpCapabilities": {"http": true}},
                    "authMethods": [{"id": "fake", "name": "Fake login", "description": "fake --login"}],
                }})),
                Some("session/new") => {
                    record(count, "new");
                    if script == "auth" && recorded(count, "new") == 1 {
                        say(
                            json!({"jsonrpc": "2.0", "id": id, "error": {"code": AUTH_REQUIRED, "message": "auth required"}}),
                        );
                    } else {
                        say(json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": session}}));
                    }
                }
                Some("session/load") => {
                    record(count, "load");
                    if message.pointer("/params/sessionId").and_then(Value::as_str)
                        == Some("saved-ok")
                    {
                        say(json!({"jsonrpc": "2.0", "id": id, "result": {}}));
                    } else {
                        say(
                            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32602, "message": "no such session"}}),
                        );
                    }
                }
                Some("session/prompt") => {
                    record(count, "prompt");
                    let prompts = recorded(count, "prompt");
                    match script {
                        "refusal" => stop(&id, "refusal"),
                        "permission" => {
                            pending_prompt = Some(id);
                            say(
                                json!({"jsonrpc": "2.0", "id": 99, "method": "session/request_permission", "params": {
                                    "sessionId": session,
                                    "toolCall": {"toolCallId": "t1", "title": "rm -rf /", "kind": "execute"},
                                    "options": [
                                        {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
                                        {"optionId": "reject", "name": "Reject", "kind": "reject_once"},
                                    ],
                                }}),
                            );
                        }
                        "slow" if prompts == 1 => pending_prompt = Some(id),
                        "exit" if spawns == 1 => std::process::exit(3),
                        _ => {
                            chunk(session, "Hell");
                            if script == "garbage" {
                                println!("this is not json");
                            }
                            chunk(session, "o");
                            stop(&id, "end_turn");
                        }
                    }
                }
                Some("session/cancel") => {
                    record(count, "cancel");
                    if let Some(id) = pending_prompt.take() {
                        stop(&id, "cancelled");
                    }
                }
                Some(_) => {}
                // A reply to our own permission request.
                None if id == json!(99) => {
                    let outcome = message
                        .pointer("/result/outcome/outcome")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string();
                    record(count, &format!("perm:{outcome}"));
                    if outcome == "selected" {
                        let option = message
                            .pointer("/result/outcome/optionId")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        chunk(session, &format!("ok:{option}"));
                        if let Some(id) = pending_prompt.take() {
                            stop(&id, "end_turn");
                        }
                    }
                }
                None => {}
            }
        }
    }

    struct Fixture {
        dir: PathBuf,
        count: PathBuf,
        asks: Receiver<PermissionAsk>,
    }

    impl Fixture {
        fn new(script: &str) -> (Self, Session) {
            let dir =
                std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let count = dir.join("count.txt");
            let exe = std::env::current_exe().unwrap();
            let test = module_path!()
                .split_once("::")
                .map_or("", |(_, rest)| rest)
                .to_string()
                + "::fake_acp_agent";
            let launch = Launch {
                name: "fake".into(),
                argv: vec![
                    exe.to_string_lossy().to_string(),
                    test,
                    "--exact".into(),
                    "--nocapture".into(),
                    "--test-threads=1".into(),
                    format!("script={script}"),
                    format!("count={}", count.display()),
                ],
            };
            let (tx, asks) = mpsc::channel();
            let session = Session::new(
                launch,
                dir.clone(),
                Box::new(move |ask| {
                    let _ = tx.send(ask);
                }),
            )
            .with_timeout(Duration::from_secs(10));
            (Self { dir, count, asks }, session)
        }

        fn count(&self, what: &str) -> usize {
            recorded(Some(&self.count), what)
        }

        fn wait_for(&self, what: &str, n: usize) -> bool {
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until {
                if self.count(what) >= n {
                    return true;
                }
                thread::sleep(Duration::from_millis(20));
            }
            false
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn launch_table_names_the_four_shapes_and_leaves_http_alone_when_unset() {
        assert_eq!(launch(None), None);
        assert_eq!(launch(Some("")), None);
        assert_eq!(launch(Some("  ")), None);
        let claude = launch(Some("claude")).unwrap();
        assert_eq!(claude.name, "claude");
        assert_eq!(
            claude.argv,
            ["npx", "-y", "@agentclientprotocol/claude-agent-acp"]
        );
        assert_eq!(launch(Some("hermes")).unwrap().argv, ["hermes", "acp"]);
        assert_eq!(launch(Some("opencode")).unwrap().argv, ["opencode", "acp"]);
        let custom = launch(Some("  my-agent --acp  --quiet ")).unwrap();
        assert_eq!(custom.name, "my-agent");
        assert_eq!(custom.argv, ["my-agent", "--acp", "--quiet"]);
    }

    /// ADR-0010 rules 4 and 5, as code: the child gets our environment as
    /// it is, with no key set, no config dir moved, and no `--bare`.
    #[test]
    fn child_command_sets_no_env_and_passes_no_bare() {
        for name in ["claude", "hermes", "opencode"] {
            let launch = launch(Some(name)).unwrap();
            let command = launch.command(Path::new("/tmp"));
            assert_eq!(command.get_envs().count(), 0, "{name} sets env");
            assert!(
                !launch.argv.iter().any(|arg| arg == "--bare"),
                "{name} passes --bare"
            );
            assert!(!launch
                .argv
                .iter()
                .any(|arg| arg.contains("CLAUDE_CONFIG_DIR") || arg.contains("ANTHROPIC_API_KEY")));
        }
    }

    #[test]
    fn happy_path_concatenates_chunks_and_records_the_session() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(session.complete("hi"), Ok("Hello".to_string()));
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        assert_eq!(saved["session_id"], "fresh-id");
        assert_eq!(saved["harness"], "fake");
        let inspect = session.inspect();
        assert!(inspect.mcp_http);
        assert_eq!(inspect.agent.as_deref(), Some("fake-agent"));
        assert!(std::fs::read_to_string(fx.dir.join(action_log::FILE))
            .unwrap()
            .contains("\"event\":\"turn\""));
        session.shutdown();
    }

    #[test]
    fn a_refusal_is_an_err() {
        let (_fx, session) = Fixture::new("refusal");
        let reply = session.complete("hi");
        assert!(
            reply.as_ref().is_err_and(|why| why.contains("refusal")),
            "{reply:?}"
        );
        session.shutdown();
    }

    #[test]
    fn garbage_between_messages_is_skipped() {
        let (_fx, session) = Fixture::new("garbage");
        assert_eq!(session.complete("hi"), Ok("Hello".to_string()));
        session.shutdown();
    }

    #[test]
    fn permission_request_reaches_the_hook_and_the_answer_completes_the_turn() {
        let (fx, session) = Fixture::new("permission");
        let session = Arc::new(session);
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete("hi"))
        };
        let ask = fx.asks.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(ask.title, "rm -rf /");
        assert_eq!(ask.kind.as_deref(), Some("execute"));
        assert_eq!(ask.options.len(), 2);
        session.answer_permission(&ask.request, "allow");
        assert_eq!(worker.join().unwrap(), Ok("ok:allow".to_string()));
        assert!(fx.wait_for("perm:selected", 1));
        session.shutdown();
    }

    #[test]
    fn a_timeout_before_the_answer_sends_the_cancelled_outcome() {
        let (fx, session) = Fixture::new("permission");
        let session = session.with_timeout(Duration::from_millis(700));
        let reply = session.complete("hi");
        assert!(reply.is_err(), "{reply:?}");
        fx.asks.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(fx.wait_for("cancel", 1));
        assert!(fx.wait_for("perm:cancelled", 1));
        session.shutdown();
    }

    #[test]
    fn a_slow_turn_is_cancelled_and_the_next_one_works() {
        let (fx, session) = Fixture::new("slow");
        let session = session.with_timeout(Duration::from_millis(500));
        let reply = session.complete("hi");
        assert!(
            reply.as_ref().is_err_and(|why| why.contains("cancelled")),
            "{reply:?}"
        );
        assert!(fx.wait_for("cancel", 1));
        assert_eq!(session.complete("again"), Ok("Hello".to_string()));
        assert_eq!(fx.count("spawn"), 1, "cancel is not a respawn");
        session.shutdown();
    }

    #[test]
    fn a_child_that_exits_mid_turn_is_respawned_on_the_next_wake() {
        let (fx, session) = Fixture::new("exit");
        assert_eq!(session.complete("hi"), Err("harness exited".to_string()));
        assert!(!session.inspect().alive);
        assert_eq!(session.complete("again"), Ok("Hello".to_string()));
        assert_eq!(fx.count("spawn"), 2);
        assert!(session.inspect().alive);
        session.shutdown();
    }

    #[test]
    fn auth_required_names_the_login_and_the_retry_gate_holds() {
        let (fx, session) = Fixture::new("auth");
        let reply = session.complete("hi");
        assert_eq!(
            reply,
            Err("harness not authenticated: run `fake --login`".to_string())
        );
        assert_eq!(session.inspect().login.as_deref(), Some("fake --login"));
        // Inside the gate: fails fast, no second session/new on the wire.
        assert!(session.complete("hi").is_err());
        assert_eq!(fx.count("new"), 1);
        session.shutdown();

        let (fx, session) = Fixture::new("auth");
        let session = session.with_auth_retry(Duration::ZERO);
        assert!(session.complete("hi").is_err());
        assert_eq!(session.complete("hi"), Ok("Hello".to_string()));
        assert_eq!(fx.count("new"), 2);
        assert_eq!(session.inspect().login, None);
        session.shutdown();
    }

    #[test]
    fn a_saved_session_is_loaded_and_a_stale_one_falls_back_to_new() {
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"session_id":"saved-ok","harness":"fake"}"#,
        )
        .unwrap();
        assert_eq!(session.complete("hi"), Ok("Hello".to_string()));
        assert_eq!(fx.count("load"), 1);
        assert_eq!(fx.count("new"), 0);
        assert_eq!(session.inspect().session_id.as_deref(), Some("saved-ok"));
        session.shutdown();

        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"session_id":"stale","harness":"fake"}"#,
        )
        .unwrap();
        assert_eq!(session.complete("hi"), Ok("Hello".to_string()));
        assert_eq!(fx.count("load"), 1);
        assert_eq!(fx.count("new"), 1);
        let saved = std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap();
        assert!(saved.contains("fresh-id"), "{saved}");
        session.shutdown();

        // Another Harness's session is not ours to load.
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"session_id":"saved-ok","harness":"other"}"#,
        )
        .unwrap();
        assert_eq!(session.complete("hi"), Ok("Hello".to_string()));
        assert_eq!(fx.count("load"), 0);
        session.shutdown();
    }

    #[test]
    fn a_missing_binary_backs_off_instead_of_respawning() {
        let dir = std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
        let launch = Launch {
            name: "nope".into(),
            argv: vec!["/nonexistent/ai-buddy-no-such-harness".into()],
        };
        let session = Session::new(launch, dir.clone(), Box::new(|_| {}));
        let first = session.complete("hi").unwrap_err();
        assert!(first.contains("could not start"), "{first}");
        let second = session.complete("hi").unwrap_err();
        assert!(second.contains("retrying in"), "{second}");
        assert_eq!(backoff(1), Duration::from_secs(5));
        assert_eq!(backoff(2), Duration::from_secs(10));
        assert_eq!(backoff(40), BACKOFF_CAP);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_second_turn_while_one_is_in_flight_is_busy() {
        let (_fx, session) = Fixture::new("slow");
        let session = Arc::new(session.with_timeout(Duration::from_secs(3)));
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete("hi"))
        };
        thread::sleep(Duration::from_millis(300));
        assert_eq!(session.complete("again"), Err("harness busy".to_string()));
        let _ = worker.join();
        session.shutdown();
    }

    #[test]
    fn turn_finished_reads_only_end_turn_as_done() {
        assert!(turn_finished(&json!({"stopReason": "end_turn"})).is_ok());
        for reason in ["refusal", "max_tokens", "max_turn_requests", "cancelled"] {
            assert!(turn_finished(&json!({"stopReason": reason})).is_err());
        }
        assert!(turn_finished(&json!({})).is_err());
    }

    #[test]
    fn login_command_prefers_the_known_fix_then_the_method_then_a_hint() {
        assert_eq!(login_command("claude", &[]), "claude /login");
        assert_eq!(
            login_command(
                "hermes",
                &[json!({"name": "Hermes", "description": "hermes login"})]
            ),
            "hermes login"
        );
        assert_eq!(
            login_command("hermes", &[json!({"name": "Hermes"})]),
            "Hermes"
        );
        assert_eq!(
            login_command("x", &[]),
            "x (run it once in a terminal and sign in)"
        );
    }
}
