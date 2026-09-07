//! The attached Harness as the Completer.
//!
//! The sibling of `model.rs`. Where that file posts a Character Prompt to a
//! chat-completions host, this one spawns the Harness in ACP mode and makes
//! every wake one `session/prompt` in one session (ADR-0008, ADR-0010). The
//! protocol itself lives in `acp_wire.rs`; this file owns the policy around
//! it: which Harness, when to spawn and respawn, what a failure means, where
//! the session id is kept, and what reaches the Action Log and the Chat
//! surface. The frame loop never sees any of it: `complete` runs on a `Slots`
//! worker.
//!
//! Authentication is the Harness's own. Nothing here sets a provider key,
//! reads a credential, or calls `authenticate`; `auth_required` becomes a
//! command the user runs in their own terminal (ADR-0010's eight rules).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use ai_buddy_core::director::Completer;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::acp_wire::{Event, Handshake, OpenError, TurnError, Wire};
use crate::action_log;

pub use crate::acp_wire::PermissionAsk;

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
/// command line of the user's own, which is how Grok Build, Copilot CLI and
/// Gemini CLI attach until they are smoked; Pi and Codex are deferred
/// (ADR-0017).
pub fn launch(value: Option<&str>) -> Option<Launch> {
    let value = value?.trim();
    let (name, argv): (&str, Vec<&str>) = match value {
        "" => return None,
        "claude" => (
            value,
            vec!["npx", "-y", "@agentclientprotocol/claude-agent-acp"],
        ),
        "hermes" => (value, vec!["hermes", "acp"]),
        "opencode" => (value, vec!["opencode", "acp"]),
        custom => {
            let argv: Vec<&str> = custom.split_whitespace().collect();
            (argv[0], argv)
        }
    };
    Some(Launch {
        name: name.to_string(),
        argv: argv.into_iter().map(str::to_string).collect(),
    })
}

pub fn from_env() -> Option<Launch> {
    launch(std::env::var(VAR).ok().as_deref())
}

impl Launch {
    /// The child, inheriting our environment untouched. ADR-0010 rules 4 and
    /// 5: no provider key, no `CLAUDE_CONFIG_DIR`, no `--bare`. A test pins it.
    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command.args(&self.argv[1..]).current_dir(cwd);
        command
    }

    fn line(&self) -> String {
        self.argv.join(" ")
    }
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

/// What `harness-session.json` holds: a pointer at the Harness's own session.
#[derive(Deserialize, Serialize)]
struct SavedSession {
    session_id: String,
    harness: String,
    agent: Option<String>,
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
    wire: Mutex<Option<Arc<Wire>>>,
    inspect: Mutex<HarnessInspect>,
}

#[derive(Default)]
struct State {
    session_id: Option<String>,
    handshake: Handshake,
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
            wire: Mutex::new(None),
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

    /// Where the Harness should be by the time the first wake arrives. Spawned
    /// so startup does not wait on `npx`; the outcome is one stderr line.
    pub fn spawn_preflight(self: &Arc<Self>) {
        let session = Arc::clone(self);
        thread::spawn(move || match session.attach() {
            Ok((_, id)) => eprintln!("harness: {} attached, session {id}", session.launch.name),
            Err(why) => eprintln!("harness: {why}; StaticDirector is in force until it answers"),
        });
    }

    /// One turn. The whole of `Completer::complete`, minus the trace.
    fn turn(&self, prompt: &str) -> Result<String, String> {
        let Ok(_turn) = self.turn.try_lock() else {
            return Err("harness busy".to_string());
        };
        let (wire, session_id) = self.attach()?;
        action_log::append(
            &self.dir,
            "prompt",
            json!({"session_id": session_id, "chars": prompt.len()}),
        );
        match wire.prompt(prompt, self.timeout) {
            Ok(text) => {
                action_log::append(&self.dir, "turn", json!({"text": text}));
                if let Ok(mut state) = self.state.lock() {
                    state.spawn_failures = 0;
                    state.spawn_wait_until = None;
                }
                Ok(text)
            }
            Err(TurnError::Lost) => {
                let why = self.lost(&wire);
                // A Harness that dies under every turn costs the same respawn
                // as one that never starts, so it pays the same backoff — else
                // every wake spawns a child that is about to die again, paced
                // only by the Director. The first death is left free: a crash
                // or a binary replaced under us is recovered from on the next
                // wake, and it is the repeat that is a pattern.
                if let Ok(mut state) = self.state.lock() {
                    state.spawn_failures += 1;
                    state.spawn_wait_until = (state.spawn_failures > 1)
                        .then(|| Instant::now() + backoff(state.spawn_failures - 1));
                }
                Err(why)
            }
            Err(TurnError::Timeout) => {
                action_log::append(&self.dir, "timeout", json!({"session_id": session_id}));
                Err(format!(
                    "harness turn exceeded {}s; cancelled",
                    self.timeout.as_secs()
                ))
            }
            Err(TurnError::Stopped(reason)) => {
                action_log::append(&self.dir, "turn", json!({"stop": reason}));
                Err(format!("harness stopped: {reason}"))
            }
            Err(TurnError::Busy) => Err("harness busy".to_string()),
            Err(TurnError::Failed(why)) => {
                action_log::append(&self.dir, "turn", json!({"error": why}));
                Err(format!("harness: {why}"))
            }
        }
    }

    /// The wire died under a turn. Say so, and let the next wake respawn.
    ///
    /// The slot is emptied here rather than left for `alive()` to notice:
    /// that flag flips only once the wire's thread has dropped its receiver,
    /// and a wake arriving before then would reuse a dead wire.
    fn lost(&self, wire: &Wire) -> String {
        wire.shutdown();
        if let Ok(mut slot) = self.wire.lock() {
            *slot = None;
        }
        self.update_inspect(|inspect| inspect.alive = false);
        "harness exited".to_string()
    }

    /// The user's answer to a forwarded permission request. Never chosen here.
    pub fn answer_permission(&self, request: &str, option: &str) {
        let Some(wire) = self.current_wire() else {
            return;
        };
        action_log::append(
            &self.dir,
            "permission_answer",
            json!({"request": request, "option": option}),
        );
        wire.answer(request, option);
    }

    /// On exit: cancel what is in flight, answer open asks `cancelled`, and
    /// kill the child.
    pub fn shutdown(&self) {
        if let Some(wire) = self.wire.lock().ok().and_then(|mut slot| slot.take()) {
            wire.shutdown();
        }
    }

    fn current_wire(&self) -> Option<Arc<Wire>> {
        self.wire
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .filter(|wire| wire.alive())
    }

    /// A live child and an open session, spawning and negotiating as needed.
    fn attach(&self) -> Result<(Arc<Wire>, String), String> {
        let mut state = self.state.lock().map_err(|_| "harness state poisoned")?;
        let wire = match self.current_wire() {
            Some(wire) => wire,
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
                    Ok(wire) => wire,
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
            return Ok((wire, id.clone()));
        }
        if let (Some(command), Some(tried)) = (&state.login, state.auth_tried) {
            if tried.elapsed() < self.auth_retry {
                return Err(not_authenticated(command));
            }
        }
        let id = self.open_session(&wire, &mut state)?;
        Ok((wire, id))
    }

    fn spawn_and_initialize(&self, state: &mut State) -> Result<Arc<Wire>, String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|why| format!("{}: {why}", self.dir.display()))?;
        let dir = self.dir.clone();
        let forward = Arc::clone(&self.forward);
        let wire = Wire::spawn(
            self.launch.command(&self.dir),
            self.attach_timeout(),
            Box::new(move |event| note_event(&dir, &forward, event)),
        )
        .map_err(|why| format!("`{}` {why}", self.launch.line()))?;
        state.handshake = wire.handshake().clone();
        // A fresh process is a fresh chance to sign in: the gate belonged to
        // the one that died.
        state.login = None;
        state.auth_tried = None;
        self.update_inspect(|inspect| {
            inspect.agent = state.handshake.agent.clone();
            inspect.mcp_http = state.handshake.mcp_http;
            inspect.alive = true;
        });
        let wire = Arc::new(wire);
        if let Ok(mut slot) = self.wire.lock() {
            *slot = Some(Arc::clone(&wire));
        }
        Ok(wire)
    }

    /// `session/load` when the Harness can and the file names this Harness,
    /// else `session/new`. Either way the file ends up naming what is open.
    fn open_session(&self, wire: &Arc<Wire>, state: &mut State) -> Result<String, String> {
        let saved = state
            .handshake
            .load_session
            .then(|| self.saved_session())
            .flatten();
        let mcp = mcp_server();
        let id = match wire.open(saved, &self.dir, mcp.clone(), self.attach_timeout()) {
            Ok(id) => id,
            Err(OpenError::Lost) => return Err(self.lost(wire)),
            Err(OpenError::AuthRequired) => {
                let command = login_command(&self.launch.name, &state.handshake);
                state.login = Some(command.clone());
                state.auth_tried = Some(Instant::now());
                self.update_inspect(|inspect| inspect.login = Some(command.clone()));
                return Err(not_authenticated(&command));
            }
            Err(OpenError::Failed(why)) => return Err(format!("session/new: {why}")),
        };
        state.session_id = Some(id.clone());
        state.login = None;
        self.update_inspect(|inspect| {
            inspect.login = None;
            inspect.session_id = Some(id.clone());
        });
        self.save_session(&id);
        action_log::append(
            &self.dir,
            "attach",
            json!({"harness": self.launch.name, "session_id": id, "mcp": mcp}),
        );
        Ok(id)
    }

    fn saved_session(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.dir.join(SESSION_FILE)).ok()?;
        let saved: SavedSession = serde_json::from_str(&text).ok()?;
        (saved.harness == self.launch.name).then_some(saved.session_id)
    }

    fn save_session(&self, id: &str) {
        let record = SavedSession {
            session_id: id.to_string(),
            harness: self.launch.name.clone(),
            agent: self.inspect().agent,
        };
        if let Ok(text) = serde_json::to_string(&record) {
            let _ = std::fs::write(self.dir.join(SESSION_FILE), format!("{text}\n"));
        }
    }
}

impl Completer for Session {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        if crate::model::tracing() {
            eprintln!("harness: prompt to {}", self.launch.name);
        }
        let reply = self.turn(prompt);
        if crate::model::tracing() {
            match &reply {
                Ok(text) => eprintln!("harness: reply {text}"),
                Err(why) => eprintln!("harness: {why}"),
            }
        }
        reply
    }
}

/// What the session stream said, into the Action Log — and, for a permission
/// request, on to the Chat surface. Runs on the wire thread.
fn note_event(dir: &Path, forward: &Forward, event: Event) {
    match event {
        Event::ToolCall {
            id,
            title,
            kind,
            status,
        } => action_log::append(
            dir,
            "tool_call",
            json!({"id": id, "title": title, "kind": kind, "status": status}),
        ),
        Event::Plan { entries } => action_log::append(dir, "plan", json!({"entries": entries})),
        Event::Usage { used, size } => {
            action_log::append(dir, "usage_update", json!({"used": used, "size": size}))
        }
        Event::Permission(ask) => {
            action_log::append(
                dir,
                "permission_request",
                json!({"request": ask.request, "title": ask.title, "kind": ask.kind}),
            );
            forward(ask);
        }
    }
}

fn not_authenticated(command: &str) -> String {
    format!("harness not authenticated: run `{command}`")
}

fn backoff(failures: u32) -> Duration {
    BACKOFF_FIRST
        .saturating_mul(1u32 << failures.saturating_sub(1).min(16))
        .min(BACKOFF_CAP)
}

/// The command that logs the user in, in the Harness's own words where it
/// has any. Claude Code's adapter reports the method but not the command.
fn login_command(name: &str, handshake: &Handshake) -> String {
    if name == "claude" {
        return "claude /login".to_string();
    }
    handshake
        .auth_methods
        .first()
        .map(|method| {
            method
                .description
                .clone()
                .unwrap_or_else(|| method.name.clone())
        })
        .unwrap_or_else(|| format!("{name} (run it once in a terminal and sign in)"))
}

/// The stdio MCP server to hand the session, when the binary can be found.
///
/// Beside the app, or wherever `AI_BUDDY_MCP_BIN` points. No loopback HTTP
/// server exists yet (#166), so a missing binary means no tools this session.
fn mcp_server() -> Option<PathBuf> {
    std::env::var_os(MCP_BIN)
        .map(PathBuf::from)
        .or_else(|| {
            let beside = std::env::current_exe().ok()?.parent()?.join("ai-buddy-mcp");
            Some(if cfg!(windows) {
                beside.with_extension("exe")
            } else {
                beside
            })
        })
        .filter(|path| path.is_file())
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
    vec![
        format!(
            "harness: {} via `{}`",
            session.launch.name,
            session.launch.line()
        ),
        match mcp_server() {
            None => {
                "harness: no ai-buddy-mcp binary found; the session gets no MCP servers".to_string()
            }
            Some(path) => format!("harness: MCP server {}", path.display()),
        },
        // Startup cannot report a spawn that has not happened: `attach` runs on
        // the preflight thread and lands after these lines. Said here so the
        // `harness:` line that follows reads as this attachment's outcome
        // rather than as unrelated noise a moment later.
        "harness: attaching now; the next `harness:` line on this stream is how it went"
            .to_string(),
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
    use serde_json::Value;
    use std::io::{BufRead, Write};
    use std::sync::mpsc::{self, Receiver};

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
        let mut session = "fresh-id".to_string();
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
                    "agentInfo": {"name": "fake-agent", "version": "0"},
                    "agentCapabilities": {"loadSession": script == "load", "mcpCapabilities": {"http": true}},
                    "authMethods": [{"id": "fake", "name": "Fake login", "description": "fake --login"}],
                }})),
                Some("session/new") => {
                    record(count, "new");
                    if script == "auth" && recorded(count, "new") == 1 {
                        say(
                            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "auth required"}}),
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
                    // The SDK routes updates by session id, so a loaded
                    // session's chunks must carry the loaded id.
                    if let Some(id) = message.pointer("/params/sessionId").and_then(Value::as_str) {
                        session = id.to_string();
                    }
                    let prompts = recorded(count, "prompt");
                    match script {
                        "refusal" => stop(&id, "refusal"),
                        "permission" => {
                            pending_prompt = Some(id);
                            say(
                                json!({"jsonrpc": "2.0", "id": 99, "method": "session/request_permission", "params": {
                                    "sessionId": &session,
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
                        "die" => std::process::exit(3),
                        _ => {
                            chunk(&session, "Hell");
                            if script == "garbage" {
                                println!("this is not json");
                            }
                            chunk(&session, "o");
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
                        chunk(&session, &format!("ok:{option}"));
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
            assert_eq!(command.get_current_dir(), Some(Path::new("/tmp")));
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
        let saved: SavedSession =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        assert_eq!(saved.session_id, "fresh-id");
        assert_eq!(saved.harness, "fake");
        assert_eq!(saved.agent.as_deref(), Some("fake-agent"));
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

    /// #437: the count `a_missing_binary_backs_off_instead_of_respawning`
    /// exercises is shared with mid-turn deaths, so a Harness that dies under
    /// every prompt is not respawned on every wake. The first death stays free
    /// — that is the test above.
    #[test]
    fn repeated_mid_turn_deaths_back_off_instead_of_respawning_every_wake() {
        let (fx, session) = Fixture::new("die");
        assert_eq!(session.complete("hi"), Err("harness exited".to_string()));
        assert_eq!(session.complete("again"), Err("harness exited".to_string()));
        assert_eq!(fx.count("spawn"), 2, "the first death is recovered from");
        let third = session.complete("third").unwrap_err();
        assert!(third.contains("retrying in"), "{third}");
        assert_eq!(fx.count("spawn"), 2, "the third wake spawned another child");
        session.shutdown();
    }

    #[test]
    fn auth_required_names_the_login_and_the_retry_gate_holds() {
        let (fx, session) = Fixture::new("auth");
        let reply = session.complete("hi");
        assert_eq!(reply, Err(not_authenticated("fake --login")));
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
    fn login_command_prefers_the_known_fix_then_the_method_then_a_hint() {
        let hint = |description: Option<&str>| Handshake {
            auth_methods: vec![crate::acp_wire::AuthHint {
                name: "Hermes".into(),
                description: description.map(str::to_string),
            }],
            ..Default::default()
        };
        assert_eq!(
            login_command("claude", &Handshake::default()),
            "claude /login"
        );
        assert_eq!(
            login_command("hermes", &hint(Some("hermes login"))),
            "hermes login"
        );
        assert_eq!(login_command("hermes", &hint(None)), "Hermes");
        assert_eq!(
            login_command("x", &Handshake::default()),
            "x (run it once in a terminal and sign in)"
        );
    }
}
