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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use ai_buddy_core::director::{Completer, Wake, WakeRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::acp_wire::{Event, Handshake, McpLaunch, OpenError, TurnError, Wire};
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

/// Respawn backoff after a wake the child could not serve: doubles from the
/// first up to the cap, so a missing binary costs one attempt every five
/// minutes, not a loop.
const BACKOFF_FIRST: Duration = Duration::from_secs(5);
const BACKOFF_CAP: Duration = Duration::from_secs(5 * 60);

/// How long a superseding wake waits for the cancelled turn to hand the lock
/// back, and how often it looks. Generous for a Harness that answers
/// `session/cancel` at all, since the loser only has a log line left to write.
const HANDOVER: Duration = Duration::from_secs(3);
const HANDOVER_POLL: Duration = Duration::from_millis(20);

/// What every caller is told when the child is gone.
const LOST: &str = "harness exited";

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
        // `@latest` is load-bearing: npx serves whatever it cached the first
        // time, and the adapter bundles the Claude Code it was built against,
        // so a cache from months ago answers every turn `API Error: 400 ...
        // does not support this model` for a model newer than that CLI —
        // which `claude update` cannot fix, because it updates a different
        // install. One registry round-trip per spawn buys a Harness that
        // matches the configured model (#514). Pinning a version drifts the
        // same way in slower motion.
        "claude" => (
            value,
            vec!["npx", "-y", "@agentclientprotocol/claude-agent-acp@latest"],
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

/// The exported variable, else the Completer source row Settings saved.
///
/// The variable outranks the file the way it does on every other Director row
/// (#272, #436). Exported-and-empty is not the same as unexported here, which
/// is where this parts company with `model::env_override`: `AI_BUDDY_HARNESS=`
/// has been the kill switch since #433, and falling through to the row would
/// spawn the very Harness the export was clearing (#452).
pub fn from_settings(saved: Option<&str>) -> Option<Launch> {
    match std::env::var(VAR) {
        Ok(exported) => launch(Some(&exported)),
        Err(_) => launch(saved),
    }
}

/// The command line the source in force would spawn, or `None` for Off.
///
/// For a window that has to say whether what is attached is what the row now
/// asks for. Compared against `HarnessInspect::command`, which is the same
/// join.
pub fn wanted_command(saved: Option<&str>) -> Option<String> {
    from_settings(saved).map(|launch| launch.line())
}

impl Launch {
    /// The child, inheriting our environment untouched. ADR-0010 rules 4 and
    /// 5: no provider key, no `CLAUDE_CONFIG_DIR`, no `--bare`. A test pins it.
    ///
    /// Own process group once `own_interrupt` has taken Ctrl+C, so a SIGINT
    /// aimed at `cargo run` does not dump inside Claude's ACP adapter.
    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command.args(&self.argv[1..]).current_dir(cwd);
        isolate_from_interrupt(&mut command);
        command
    }

    fn line(&self) -> String {
        self.argv.join(" ")
    }
}

/// Tests isolate without installing a `ctrlc` handler. Production stays
/// false until `own_interrupt` — a failed handler must not orphan a tree
/// Ctrl+C can no longer reach.
static INTERRUPT_OWNED: AtomicBool = AtomicBool::new(cfg!(test));
static INTERRUPT_QUITTING: AtomicBool = AtomicBool::new(false);

/// Isolation is on: Ctrl+C is ours, so the child may leave this process group.
pub fn own_interrupt() {
    INTERRUPT_OWNED.store(true, Ordering::SeqCst);
}

/// True on the second Ctrl+C, which should `exit` rather than nest `shutdown`.
pub fn interrupt_already_quitting() -> bool {
    INTERRUPT_QUITTING.swap(true, Ordering::SeqCst)
}

fn isolate_from_interrupt(command: &mut Command) {
    apply_isolation(command, INTERRUPT_OWNED.load(Ordering::SeqCst));
}

fn apply_isolation(command: &mut Command, isolate: bool) {
    if !isolate {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP is not a Unix process-group twin.
        // Grandchildren still need a Job Object if they linger; this PR
        // has not smoked Ctrl+C on Windows.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
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

/// What the Chat surface is told about a forwarded permission request.
///
/// `Settled` exists because the ask goes to every open surface and only one of
/// them takes the click: without it the others keep offering live buttons on a
/// question already answered, cancelled, or dead with its turn.
pub enum Permission {
    Ask(PermissionAsk),
    /// A request that is no longer answerable, and the option that won it —
    /// `None` when nothing was picked and the turn simply ended.
    Settled {
        request: String,
        option: Option<String>,
    },
}

type Forward = Box<dyn Fn(Permission) + Send + Sync>;

/// One ACP session for the app's lifetime, shared by every Instance.
pub struct Session {
    launch: Launch,
    dir: PathBuf,
    forward: Arc<Forward>,
    timeout: Duration,
    auth_retry: Duration,
    backoff_first: Duration,
    /// One prompt in flight. A newer wake cancels the turn holding this and
    /// takes it; only the wake that may not do that is told "harness busy",
    /// and nothing is ever queued (ADR-0008, ADR-0016).
    turn: Mutex<()>,
    /// Whether the turn holding `turn` answers something the user did. Beside
    /// the lock rather than inside it because it is read exactly when the lock
    /// cannot be taken, which is the moment the ordering rule is decided.
    serving_reactive: AtomicBool,
    /// Bookkeeping, and the blocking `initialize`/`session/*` hop under it, so
    /// two wakes cannot open two sessions.
    state: Mutex<State>,
    /// Separate from `state` so `shutdown` never waits on an attach in flight.
    wire: Mutex<Option<Arc<Wire>>>,
    inspect: Mutex<HarnessInspect>,
    /// Off drops the handle while `spawn_preflight` may still be opening a
    /// wire. `shutdown` clears this first so a spawn that lands afterwards
    /// kills the child instead of storing it.
    wanted: AtomicBool,
}

#[derive(Default)]
struct State {
    session_id: Option<String>,
    /// Whether `session_id` came from `session/load` and has not served a
    /// turn yet — the one condition #448's reopen answers to.
    loaded: bool,
    handshake: Handshake,
    login: Option<String>,
    auth_tried: Option<Instant>,
    spawn_failures: u32,
    spawn_wait_until: Option<Instant>,
}

impl State {
    /// The child answered, so no earlier death is a death in a row any more.
    /// Every outcome but a loss proves it: a stop reason, and even a turn we
    /// gave up waiting on, came back from a process that is still there.
    fn answered(&mut self) {
        self.spawn_failures = 0;
        self.spawn_wait_until = None;
    }
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
            backoff_first: BACKOFF_FIRST,
            turn: Mutex::new(()),
            serving_reactive: AtomicBool::new(false),
            state: Mutex::new(State::default()),
            wire: Mutex::new(None),
            inspect: Mutex::new(inspect),
            wanted: AtomicBool::new(true),
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

    #[cfg(test)]
    pub fn with_backoff(mut self, first: Duration) -> Self {
        self.backoff_first = first;
        self
    }

    /// The wait a wake the child could not serve buys, doubling to the cap.
    fn backoff(&self, failures: u32) -> Duration {
        self.backoff_first
            .saturating_mul(1u32 << failures.saturating_sub(1).min(16))
            .min(BACKOFF_CAP)
    }

    /// One counter and one offset for every way a wake goes unserved: a spawn
    /// that never started, an `initialize` that never came back, and a child
    /// that died under a turn all cost the same respawn, so reading the ladder
    /// at two offsets would price the same failure twice (#437).
    fn charge_loss(&self, state: &mut State) {
        state.spawn_failures += 1;
        state.spawn_wait_until = Some(Instant::now() + self.backoff(state.spawn_failures));
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

    /// Take the turn lock from the wake in flight by cancelling it, or `None`
    /// for the one wake that may not.
    ///
    /// ADR-0016's newest-wins reaches the Harness here. The rule was
    /// implemented in the HTTP Completer alone, so every Poke arriving under a
    /// turn — five seconds wide, and a proactive tick is often in one — was
    /// refused and the buddy never reacted (#472). The protocol half already
    /// existed: `session/cancel` and the `cancelled` stop reason are how a
    /// timeout is handled. This is only the trigger, and it stays one session
    /// (ADR-0008): a cancel and a resend on the same conversation.
    ///
    /// A proactive wake never displaces a reactive one — that a Poke gives way
    /// to the next ambient tick would be worse than today. It is refused
    /// instead, which costs nothing: Static weights are the Director whenever
    /// a session call does not land.
    ///
    /// The wait is bounded because the cancel is advisory: a Harness that
    /// ignores it costs one refused wake rather than a worker parked on the
    /// lock until the turn times out.
    fn supersede(&self, request: &WakeRequest) -> Option<MutexGuard<'_, ()>> {
        if !request.reactive && self.serving_reactive.load(Ordering::SeqCst) {
            return None;
        }
        self.current_wire()?.cancel();
        let until = Instant::now() + HANDOVER;
        loop {
            if let Ok(turn) = self.turn.try_lock() {
                return Some(turn);
            }
            if Instant::now() >= until {
                return None;
            }
            thread::sleep(HANDOVER_POLL);
        }
    }

    /// One turn. The whole of `Completer::complete`, minus the trace.
    fn turn(&self, request: &WakeRequest) -> Result<String, String> {
        let _turn = match self.turn.try_lock() {
            Ok(turn) => turn,
            Err(_) => match self.supersede(request) {
                Some(turn) => turn,
                None => return Err(self.refused(request, "harness busy")),
            },
        };
        self.serving_reactive
            .store(request.reactive, Ordering::SeqCst);
        let (session_id, outcome) = self.attempt(request)?;
        // #448: a `session/load` answered with a success the Harness could not
        // honour leaves an id nothing can be prompted on, and the only place it
        // says so is the turn. So the first turn on a loaded session is the
        // evidence the load was not: throw the id away, open a fresh session,
        // and ask once more. `reopen_loaded` clears the flag it reads, so the
        // second answer is the caller's however it turns out — a Harness that
        // refuses everything cannot ping-pong here.
        //
        // A loss is not one of these: the child is gone rather than the
        // session, and `lost` has already charged the respawn. Nor is a
        // timeout, which says nothing about the id and would spend the
        // Completer's budget twice.
        let (session_id, outcome) = match &outcome {
            Err(TurnError::Stopped(_) | TurnError::Failed(_)) if self.reopen_loaded() => {
                self.attempt(request)?
            }
            _ => (session_id, outcome),
        };
        match outcome {
            Ok(text) => {
                action_log::append(&self.dir, "turn", json!({"text": text}));
                Ok(text)
            }
            Err(TurnError::Lost) => Err(LOST.to_string()),
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

    /// One `session/prompt` on the session `attach` hands over: the id it went
    /// to, and what came back.
    ///
    /// Split out of `turn` for #448's one retry, which has to pay the same
    /// bookkeeping the first attempt did rather than have the caller remember
    /// to. `Err` is a wake that never reached the wire, already refused.
    fn attempt(
        &self,
        request: &WakeRequest,
    ) -> Result<(String, Result<String, TurnError>), String> {
        let (wire, session_id) = self.attach().map_err(|why| self.refused(request, &why))?;
        // The Instance and the wake kind come through the seam rather than
        // from anything here: one session serves every buddy (ADR-0008), so
        // the session id alone cannot say whose wake this is (#435).
        action_log::append(
            &self.dir,
            "prompt",
            json!({
                "session_id": session_id,
                "instance": request.instance,
                "wake": wake_kind(request.reactive),
                "chars": request.prompt.len(),
            }),
        );
        let outcome = wire.prompt(&request.prompt, self.timeout);
        // Before the outcome is dressed for the caller: a child that dies under
        // every turn is respawned on every wake unless the death pays the same
        // backoff a failed spawn does, and a child that is answering must not
        // carry a death from hours ago (#437).
        if let Ok(mut state) = self.state.lock() {
            match &outcome {
                Err(TurnError::Lost) => self.lost(&wire, &mut state),
                _ => state.answered(),
            }
            // A finished turn is the proof `session/load` was not, so any
            // later failure on this session is the Harness's own answer (#448).
            if outcome.is_ok() {
                state.loaded = false;
            }
        }
        Ok((session_id, outcome))
    }

    /// Drop a session that came from `session/load`, so the next `attach`
    /// opens a fresh one. Whether there was one to drop (#448).
    ///
    /// Only a loaded id earns it: a `session/new` the Harness refuses is the
    /// Harness answering, not a stale pointer, and reopening that would loop.
    /// The session file goes first, which is both how the reopen is kept from
    /// loading the same id straight back and how a restart is kept from
    /// resurrecting it.
    fn reopen_loaded(&self) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if !state.loaded {
            return false;
        }
        state.loaded = false;
        state.session_id = None;
        let _ = std::fs::remove_file(self.dir.join(SESSION_FILE));
        self.update_inspect(|inspect| inspect.session_id = None);
        true
    }

    /// A wake that never reached `session/prompt`: the Harness was busy with
    /// another, or there was no session to send it to.
    ///
    /// Logged because the Shell writes a `parsed` line for every reply it
    /// takes, this refusal included — and a `parsed` with no line of its own
    /// to join would read as the outcome of whichever wake was logged last.
    /// Returns `why` so the one call site stays one line (#435).
    fn refused(&self, request: &WakeRequest, why: &str) -> String {
        action_log::append(
            &self.dir,
            "refused",
            json!({
                "instance": request.instance,
                "wake": wake_kind(request.reactive),
                "why": why,
            }),
        );
        why.to_string()
    }

    /// The wire died. Drop it, charge the loss, and let a later wake respawn.
    ///
    /// Every path that finds the child gone comes through here — a turn, and
    /// `session/new` on a child that answered `initialize` and then exited —
    /// so this is the one place the backoff can be charged without a caller
    /// forgetting to (#437).
    ///
    /// The slot is emptied here rather than left for `alive()` to notice:
    /// that flag flips only once the wire's thread has dropped its receiver,
    /// and a wake arriving before then would reuse a dead wire.
    fn lost(&self, wire: &Wire, state: &mut State) {
        wire.shutdown();
        if let Ok(mut slot) = self.wire.lock() {
            *slot = None;
        }
        self.update_inspect(|inspect| inspect.alive = false);
        self.charge_loss(state);
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

    /// Cancel in-flight work, kill the child, and wait until it is reaped.
    ///
    /// The child is in its own process group, so ending this process does not
    /// take it with us; the wait is what does. `npx` does not reliably die on
    /// stdin EOF (`acp_wire`), which is why this kills rather than hanging up.
    pub fn shutdown(&self) {
        self.wanted.store(false, Ordering::SeqCst);
        let Some(wire) = self.wire.lock().ok().and_then(|mut slot| slot.take()) else {
            return;
        };
        wire.shutdown();
        if !wire.wait_for_exit(REAP) {
            eprintln!(
                "harness: `{}` was still running {}s after shutdown",
                self.launch.line(),
                REAP.as_secs()
            );
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
        if !self.wanted.load(Ordering::SeqCst) {
            return Err("harness detached".to_string());
        }
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
                        self.charge_loss(&mut state);
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
        if !self.wanted.load(Ordering::SeqCst) {
            wire.shutdown();
            return Err("harness detached".to_string());
        }
        if let Ok(mut slot) = self.wire.lock() {
            *slot = Some(Arc::clone(&wire));
        }
        if !self.wanted.load(Ordering::SeqCst) {
            self.shutdown();
            return Err("harness detached".to_string());
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
        let id = match wire.open(saved.clone(), &self.dir, mcp.clone(), self.attach_timeout()) {
            Ok(id) => id,
            Err(OpenError::Lost) => {
                self.lost(wire, state);
                return Err(LOST.to_string());
            }
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
        // `Wire::open` hands back the id it was asked to load and falls through
        // to `session/new` on a refusal, so the ids matching is exactly "the
        // load was answered" — which is all #448's reopen may act on.
        state.loaded = saved.as_deref() == Some(id.as_str());
        state.login = None;
        self.update_inspect(|inspect| {
            inspect.login = None;
            inspect.session_id = Some(id.clone());
        });
        self.save_session(&id);
        action_log::append(
            &self.dir,
            "attach",
            json!({"harness": self.launch.name, "session_id": id, "mcp": mcp.as_ref().map(McpLaunch::line)}),
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
    fn complete(&self, request: &WakeRequest) -> Result<String, String> {
        if crate::model::tracing() {
            eprintln!("harness: prompt to {}", self.launch.name);
        }
        let reply = self.turn(request);
        if crate::model::tracing() {
            match &reply {
                Ok(text) => eprintln!("harness: reply {text}"),
                Err(why) => eprintln!("harness: {why}"),
            }
        }
        reply
    }
}

/// The one prompt the probe sends.
///
/// Shaped like the last line of a Character Prompt — a Behavior name, a bar,
/// then dialogue — because whether a real Harness's reply comes back in that
/// shape is the fact the probe exists to report. Asked for explicitly rather
/// than left to the Harness's own idea of a good answer, so a reply that does
/// not parse is the Harness's doing and not the prompt's.
const PROBE_PROMPT: &str =
    "Reply with exactly this one line and nothing else: Wave | Hello from the probe.";

/// How long shutdown waits for the child to be reaped before saying so.
const REAP: Duration = Duration::from_secs(2);

/// Attach the configured Harness and run one turn, with no overlay.
///
/// `scripts/probe-harness.sh` is the face of this and `model::run_probe` is
/// the sibling it is shaped after: same env as `cargo run`, no window, an
/// exit code a script can read. The code splits on the two phases the probe
/// prints: 2 is never having asked — nothing configured, no binary, not
/// signed in — 1 is asked and not answered, and 0 is `end_turn`.
///
/// No credential is read, printed or set. A Harness that is not signed in
/// comes back as the command the user runs in their own terminal, which is
/// ADR-0010's rule and the whole of what the probe does about it.
pub fn run_probe() -> i32 {
    // No settings file on this path, and `dev_flags::seed` is where the
    // exported timeout is read (#273).
    crate::dev_flags::seed(&crate::settings::Settings::default());
    // `None` rather than the saved row: the probe loads no settings file, so
    // the exported variable is the only source it has (#436).
    let Some(launch) = from_settings(None) else {
        eprintln!("probe-harness: {VAR} is unset, so there is no Harness to attach");
        return 2;
    };
    let session = Session::new(
        launch,
        // The probe's own folder under the app's, which keeps the session file
        // and the Action Log out of a real install's — a probe is still the way
        // to reach a Harness that resumes badly (#448), so it wants both of its
        // own. Memory is not isolated by this and cannot be: the MCP server
        // resolves it from the data folder, not from the session's cwd, so a
        // `remember` during a probe writes the real `memory.md`.
        ai_buddy_core::memory::data_dir().join("probe"),
        // Named, never answered: only a click on the Chat surface may answer a
        // permission request (ADR-0017), and the probe has no surface. The ask
        // then times out with the turn, which is itself the report. A
        // settlement earns no line, because the only one a probe can reach is
        // that timeout.
        Box::new(|permission| {
            if let Permission::Ask(ask) = permission {
                println!("  permission   {} [{}]", ask.title, ask.request);
            }
        }),
    );
    let code = probe(&session);
    session.shutdown();
    code
}

/// `run_probe` minus the environment, so the fake agent can run the whole of
/// it in a test.
fn probe(session: &Session) -> i32 {
    println!("probe-harness");
    println!("  harness      {}", session.launch.name);
    println!("  command      {}", session.launch.line());
    println!("  dir          {}", session.dir.display());
    println!(
        "  mcp          {}",
        mcp_server().map_or_else(|| "none".to_string(), |launch| launch.line())
    );
    println!(
        "  timeout      turn {}s, attach {}s",
        session.timeout.as_secs(),
        session.attach_timeout().as_secs()
    );
    println!();

    println!("attach");
    let session_id = match session.attach() {
        Ok((_, id)) => id,
        // Nothing was asked, so this is configuration and not a turn: a
        // missing binary, a Harness that wants a login, a `session/new` the
        // Harness refused. The message says which; the code says only that
        // the prompt below never went out.
        Err(why) => {
            println!("  {why}");
            return 2;
        }
    };
    let handshake = session
        .state
        .lock()
        .map(|state| state.handshake.clone())
        .unwrap_or_default();
    let methods: Vec<&str> = handshake
        .auth_methods
        .iter()
        .map(|method| method.name.as_str())
        .collect();
    println!(
        "  agent        {}",
        handshake.agent.as_deref().unwrap_or("unnamed")
    );
    println!("  loadSession  {}", handshake.load_session);
    println!("  mcp http     {}", handshake.mcp_http);
    println!(
        "  authMethods  {}",
        if methods.is_empty() {
            "none".to_string()
        } else {
            methods.join(", ")
        }
    );
    println!("  session      {session_id}");
    println!();

    println!("turn");
    println!("  prompt       {PROBE_PROMPT}");
    match session.turn(&WakeRequest {
        prompt: PROBE_PROMPT.to_string(),
        // Reactive, because a probe is someone asking on purpose. The Instance
        // is named for the probe so the line it leaves in the Action Log cannot
        // be read as a buddy's own wake (#435).
        instance: "probe".to_string(),
        reactive: true,
    }) {
        Ok(text) => {
            println!("  stop         end_turn");
            println!("  reply        {text}");
            // Reported, not part of the verdict: whether a model obeys a
            // one-line format is the Director's problem, and a turn that
            // reached `end_turn` proved the wire either way.
            match ai_buddy_core::director::parse_proposal(&text) {
                Ok(proposal) => println!(
                    "  proposal     {} | {}",
                    proposal.behavior,
                    proposal.dialogue.as_deref().unwrap_or("(no dialogue)")
                ),
                Err(_) => println!("  proposal     no, the first line is not a Behavior name"),
            }
            0
        }
        Err(why) => {
            println!("  {why}");
            1
        }
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
            forward(Permission::Ask(ask));
        }
        Event::PermissionSettled { request, option } => {
            forward(Permission::Settled { request, option })
        }
    }
}

/// The Action Log line for what one reply parsed to.
///
/// Written by the Shell where it takes the wake out of `Slots`, because
/// `crates/core` parses and does no I/O. A Harness session writes beside
/// that session; an HTTP wake writes to the same data dir Memory uses (#435).
pub fn note_parsed(instance: &str, wake: &Wake, reactive: bool, near_miss: Option<&str>) {
    let dir = attached()
        .map(|session| session.dir.clone())
        .unwrap_or_else(ai_buddy_core::memory::data_dir);
    action_log::append(
        &dir,
        "parsed",
        parsed_fields(instance, wake, reactive, near_miss),
    );
}

/// The four answers the Shell has to "what did the reply parse to".
///
/// A Near Miss is its own outcome and not `speech`, though it arrives as
/// speech: a Character that declares `prowl` and a model that answers `prowll`
/// is a contract miss the log has to be able to show, which is what CONTEXT.md
/// asks and what the trace flag was the only witness to (#243).
///
/// `failed` says only that no reply was usable. Why is already a line of its
/// own — `refused`, `timeout`, or a `turn` carrying the stop reason — written
/// where the failure was seen.
///
/// The wake kind rides along so a reader can join this to the `prompt` or
/// `refused` line for the same wake.
fn parsed_fields(instance: &str, wake: &Wake, reactive: bool, near_miss: Option<&str>) -> Value {
    let (result, behavior) = match (near_miss, wake) {
        (Some(named), _) => ("near_miss", Some(named)),
        (None, Wake::Proposed(proposal)) if !proposal.behavior.is_empty() => {
            ("proposal", Some(proposal.behavior.as_str()))
        }
        (None, Wake::Proposed(_)) => ("speech", None),
        (None, Wake::Failed) => ("failed", None),
    };
    json!({
        "instance": instance,
        "wake": wake_kind(reactive),
        "result": result,
        "behavior": behavior,
    })
}

/// The Action Log's word for each of ADR-0008's two wake kinds. Proactive is
/// the wake policy's word; Ambient in the glossary is Ambient Capture.
fn wake_kind(reactive: bool) -> &'static str {
    if reactive {
        "reactive"
    } else {
        "proactive"
    }
}

fn not_authenticated(command: &str) -> String {
    format!("harness not authenticated: run `{command}`")
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

/// The stdio MCP server to hand the session, when one can be launched.
///
/// Beside the app, or wherever `AI_BUDDY_MCP_BIN` points. No loopback HTTP
/// server exists yet (#166), so a missing binary means no tools this session
/// unless the app binary itself can speak MCP on stdio.
fn mcp_server() -> Option<McpLaunch> {
    let env_bin = std::env::var_os(MCP_BIN).map(PathBuf::from);
    mcp_launch(env_bin.as_deref(), std::env::current_exe().ok()?.as_path())
}

fn mcp_launch(env_bin: Option<&Path>, current_exe: &Path) -> Option<McpLaunch> {
    if let Some(path) = env_bin.filter(|path| path.is_file()) {
        return Some(McpLaunch {
            path: path.to_path_buf(),
            args: Vec::new(),
        });
    }
    let beside = current_exe.parent()?.join("ai-buddy-mcp");
    let sibling = if cfg!(windows) {
        beside.with_extension("exe")
    } else {
        beside
    };
    if sibling.is_file() {
        return Some(McpLaunch {
            path: sibling,
            args: Vec::new(),
        });
    }
    // Sibling / env still win; this is how `cargo run` and a bundle with no
    // sidecar still hand the Harness a server. #166 is the loopback-HTTP follow-up.
    (current_exe.file_stem()? == "ai-buddy").then(|| McpLaunch {
        path: current_exe.to_path_buf(),
        args: vec!["--mcp-stdio".into()],
    })
}

static ATTACHED: Mutex<Option<Arc<Session>>> = Mutex::new(None);
static ATTACH_STARTED: AtomicBool = AtomicBool::new(false);

/// Read the source once — the variable, else `saved` from Settings — and hold
/// the Session until Off or process exit.
///
/// A process global rather than a field threaded through `DirectorSettings`:
/// the session is one per app (ADR-0008), and a Retarget from Settings
/// rebuilds `DirectorSettings` from scratch, which would drop a field. Switching
/// to a different Harness still waits for the next launch (#436). Off is
/// `detach` (#500).
pub fn attach(saved: Option<String>, forward: Forward) -> Option<Arc<Session>> {
    let mut slot = ATTACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if ATTACH_STARTED.swap(true, Ordering::SeqCst) {
        return slot.clone();
    }
    *slot = from_settings(saved.as_deref()).map(|launch| {
        Arc::new(Session::new(
            launch,
            ai_buddy_core::memory::data_dir(),
            forward,
        ))
    });
    slot.clone()
}

pub fn attached() -> Option<Arc<Session>> {
    ATTACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Drop the attached handle so the HTTP Completer is the mind. #500.
pub fn detach() {
    let Some(session) = ATTACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    else {
        return;
    };
    session.shutdown();
    eprintln!("harness: detached; HTTP Completer is the Director's mind");
}

/// Whether an attached Harness is actually answering, not merely configured.
///
/// `attach` holds the handle until Off or process exit whether or not the child
/// ever spawned, so a source row naming a Harness this machine has not got
/// leaves a `Some` that never answers a wake — and `ModelDirector` has already
/// fallen back to Static. Settings asks this rather than `attached`, because
/// freezing the HTTP rows on a Harness that never started leaves no reachable
/// Completer at all and no way back but a hand-edit of the file (#452).
///
/// A wire lost mid-session reads the same way: `lost` clears the flag, and the
/// next wake respawns.
pub fn driving() -> bool {
    attached().is_some_and(|session| session.inspect().alive)
}

/// What `startup_lines` says about the attachment, if there is one.
///
/// `spawning` is whether one is actually coming: the Director's own switch
/// gates `spawn_preflight`, and a line promising a spawn that the switch has
/// already refused is worse than no line.
pub fn startup_lines(spawning: bool) -> Vec<String> {
    let Some(session) = attached() else {
        return Vec::new();
    };
    let mut lines = vec![
        format!(
            "harness: {} via `{}`",
            session.launch.name,
            session.launch.line()
        ),
        match mcp_server() {
            None => {
                "harness: no ai-buddy-mcp binary found; the session gets no MCP servers".to_string()
            }
            Some(launch) => format!("harness: MCP server {}", launch.line()),
        },
    ];
    // Startup cannot report a spawn that has not happened: `attach` runs on the
    // preflight thread and lands after these lines. Said here so the `harness:`
    // line that follows reads as this attachment's outcome rather than as
    // unrelated noise a moment later.
    if spawning {
        lines.push(
            "harness: attaching now; the next `harness:` line on this stream is how it went"
                .to_string(),
        );
    }
    lines
}

pub fn shutdown() {
    if let Some(session) = attached() {
        session.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_buddy_core::engine::BehaviorProposal;
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
                    "agentCapabilities": {"loadSession": script.starts_with("load"), "mcpCapabilities": {"http": true}},
                    "authMethods": [{"id": "fake", "name": "Fake login", "description": "fake --login"}],
                }})),
                Some("session/new") => {
                    record(count, "new");
                    if script == "die-opening" {
                        std::process::exit(3);
                    }
                    if script == "auth" && recorded(count, "new") == 1 {
                        say(
                            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "auth required"}}),
                        );
                    } else {
                        // A new session is a new id. Without the reset the fake
                        // would hand back whichever id the last prompt named,
                        // and #448's reopen could not be told apart from the
                        // dead session it replaced.
                        session = "fresh-id".to_string();
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
                        // #448: the session the load claimed to restore is
                        // not there, so the first prompt refuses and the one
                        // after the reopen is served.
                        "load-dead" if prompts == 1 => stop(&id, "refusal"),
                        "load-refusal" => stop(&id, "refusal"),
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
        forwarded: Receiver<Permission>,
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
            let (tx, forwarded) = mpsc::channel();
            let session = Session::new(
                launch,
                dir.clone(),
                Box::new(move |permission| {
                    let _ = tx.send(permission);
                }),
            )
            .with_timeout(Duration::from_secs(10));
            (
                Self {
                    dir,
                    count,
                    forwarded,
                },
                session,
            )
        }

        /// The next forwarded ask, or a panic naming what came instead.
        fn ask(&self) -> PermissionAsk {
            match self.forwarded.recv_timeout(Duration::from_secs(5)) {
                Ok(Permission::Ask(ask)) => ask,
                other => panic!("expected an ask, got {:?}", other.map(|_| "settled")),
            }
        }

        /// The next forwarded settlement: the request, and what won it.
        fn settled(&self) -> (String, Option<String>) {
            match self.forwarded.recv_timeout(Duration::from_secs(5)) {
                Ok(Permission::Settled { request, option }) => (request, option),
                other => panic!("expected a settlement, got {:?}", other.map(|_| "ask")),
            }
        }

        fn count(&self, what: &str) -> usize {
            recorded(Some(&self.count), what)
        }

        /// Every Action Log line of one kind, oldest first.
        fn events(&self, event: &str) -> Vec<Value> {
            let text = std::fs::read_to_string(self.dir.join(action_log::FILE)).unwrap_or_default();
            text.lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .filter(|line| line["event"] == json!(event))
                .collect()
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

    /// One reactive wake for `buddy-1`, which is every turn a test sends.
    fn asking(prompt: &str) -> WakeRequest {
        WakeRequest {
            prompt: prompt.to_string(),
            instance: "buddy-1".to_string(),
            reactive: true,
        }
    }

    /// The same wake, arriving on the Director's own backoff rather than
    /// because the user did something. ADR-0008 names the two kinds.
    fn ambient(prompt: &str) -> WakeRequest {
        WakeRequest {
            reactive: false,
            ..asking(prompt)
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
            ["npx", "-y", "@agentclientprotocol/claude-agent-acp@latest"]
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

    /// Production change that would fail this: the child stays in the app's
    /// process group, so Ctrl+C SIGINTs Claude's adapter and it dumps
    /// `Query closed before response received` on the way down.
    #[cfg(unix)]
    #[test]
    fn the_harness_child_is_not_in_the_app_process_group() {
        let launch = Launch {
            name: "sleep".into(),
            argv: vec!["/bin/sleep".into(), "8".into()],
        };
        let mut command = launch.command(Path::new("/tmp"));
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        let mut child = command.spawn().expect("sleep");
        let child_pgid = pgid_of(child.id()).expect("child pgid");
        let app_pgid = pgid_of(std::process::id()).expect("app pgid");
        let _ = child.kill();
        let _ = child.wait();
        assert_ne!(
            child_pgid, app_pgid,
            "Ctrl+C in the terminal would SIGINT the Harness"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_unowned_interrupt_leaves_the_child_in_the_app_group() {
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("8");
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        apply_isolation(&mut command, false);
        let mut child = command.spawn().expect("sleep");
        let child_pgid = pgid_of(child.id()).expect("child pgid");
        let app_pgid = pgid_of(std::process::id()).expect("app pgid");
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(
            child_pgid, app_pgid,
            "a failed ctrlc handler must not orphan a tree Ctrl+C can no longer reap"
        );
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_kills_the_harness_process_group() {
        let launch = Launch {
            name: "shell".into(),
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                r#"trap "" HUP; sleep 30 & wait"#.into(),
            ],
        };
        let mut command = launch.command(Path::new("/tmp"));
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        let mut child = command.spawn().expect("sh");
        let pgid = pgid_of(child.id()).expect("child pgid");
        let members = wait_for_group_members(pgid, 2);
        crate::acp_wire::kill_harness_tree(child.id());
        let _ = child.kill();
        let _ = child.wait();
        let until = Instant::now() + Duration::from_millis(500);
        let lingering = loop {
            let still: Vec<_> = members
                .iter()
                .copied()
                .filter(|&pid| still_running(pid))
                .collect();
            if still.is_empty() || Instant::now() >= until {
                break still;
            }
            thread::sleep(Duration::from_millis(20));
        };
        assert!(
            lingering.is_empty(),
            "grandchildren survived a direct kill: {lingering:?}"
        );
    }

    #[cfg(unix)]
    fn pgid_of(pid: u32) -> Option<i32> {
        let n = unsafe { libc::getpgid(pid as libc::pid_t) };
        (n >= 0).then_some(n)
    }

    #[cfg(unix)]
    fn alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    /// `kill(pid, 0)` is true for a zombie. SIGKILL'd grandchildren show up
    /// that way until init reaps them; they are not still running.
    #[cfg(unix)]
    fn still_running(pid: u32) -> bool {
        let output = std::process::Command::new("ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .ok();
        let Some(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        matches!(
            String::from_utf8_lossy(&output.stdout)
                .chars()
                .find(|c| !c.is_whitespace()),
            Some(c) if c != 'Z'
        )
    }

    #[cfg(unix)]
    fn live_pids_in_group(pgid: i32) -> Vec<u32> {
        let output = std::process::Command::new("ps")
            .args(["-axo", "pid=,pgid="])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let mut cols = line.split_whitespace();
                let pid: u32 = cols.next()?.parse().ok()?;
                let group: i32 = cols.next()?.parse().ok()?;
                (group == pgid && alive(pid)).then_some(pid)
            })
            .collect()
    }

    #[cfg(unix)]
    fn wait_for_group_members(pgid: i32, n: usize) -> Vec<u32> {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let pids = live_pids_in_group(pgid);
            if pids.len() >= n {
                return pids;
            }
            if Instant::now() >= until {
                panic!("group {pgid} never grew to {n}: {pids:?}");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn happy_path_concatenates_chunks_and_records_the_session() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
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

    /// Off may race `spawn_preflight`. A spawn that lands after `shutdown`
    /// must not store a live child the HTTP Completer then cannot see.
    #[test]
    fn shutdown_refuses_a_later_turn() {
        let (_fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
        session.shutdown();
        assert_eq!(
            session.complete(&asking("again")),
            Err("harness detached".to_string())
        );
    }

    /// #435: one session serves every buddy, so `session_id` cannot answer
    /// "which Instance woke, and did the user ask for it". Both come through
    /// the `WakeRequest` or not at all.
    #[test]
    fn the_prompt_event_names_the_instance_and_the_wake_kind() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
        let proactive = WakeRequest {
            reactive: false,
            ..asking("nobody asked")
        };
        assert_eq!(session.complete(&proactive), Ok("Hello".to_string()));
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        assert_eq!(prompts[0]["instance"], json!("buddy-1"));
        assert_eq!(prompts[0]["wake"], json!("reactive"));
        assert_eq!(prompts[0]["chars"], json!(2));
        assert_eq!(prompts[1]["wake"], json!("proactive"));
        assert_eq!(prompts[1]["session_id"], json!("fresh-id"));
    }

    /// #435: without this the log said a prompt went out and never what came
    /// of it, so "did we take what the Harness proposed" needed the trace flag.
    #[test]
    fn the_parsed_event_says_what_the_reply_became() {
        let spoke = |line: &str| {
            Wake::Proposed(BehaviorProposal {
                behavior: String::new(),
                dialogue: Some(line.to_string()),
            })
        };

        let named = parsed_fields(
            "buddy-1",
            &Wake::Proposed(BehaviorProposal {
                behavior: "prowl".to_string(),
                dialogue: Some("mine now".to_string()),
            }),
            true,
            None,
        );
        assert_eq!(named["instance"], json!("buddy-1"));
        assert_eq!(named["wake"], json!("reactive"));
        assert_eq!(named["result"], json!("proposal"));
        assert_eq!(named["behavior"], json!("prowl"));

        // An empty name is the Engine's "talk and speak": the model chose to
        // talk rather than name a Behavior.
        let talked = parsed_fields("buddy-1", &spoke("hello?"), false, None);
        assert_eq!(talked["wake"], json!("proactive"));
        assert_eq!(talked["result"], json!("speech"));
        assert_eq!(talked["behavior"], json!(null));

        // #243: the same shape as speech on the wire, and a different thing —
        // the name it named is what makes it readable as a miss.
        let missed = parsed_fields("buddy-1", &spoke("prowll"), true, Some("prowll"));
        assert_eq!(missed["result"], json!("near_miss"));
        assert_eq!(missed["behavior"], json!("prowll"));

        let failed = parsed_fields("buddy-1", &Wake::Failed, true, None);
        assert_eq!(failed["result"], json!("failed"));
        assert_eq!(failed["behavior"], json!(null));
    }

    #[test]
    fn a_refusal_is_an_err() {
        let (fx, session) = Fixture::new("refusal");
        let reply = session.complete(&asking("hi"));
        assert!(
            reply.as_ref().is_err_and(|why| why.contains("refusal")),
            "{reply:?}"
        );
        // #448: only a loaded id is reopened. This session came from
        // `session/new`, so the refusal is the Harness's answer to the prompt.
        assert_eq!(fx.count("new"), 1);
        assert_eq!(fx.count("prompt"), 1);
        session.shutdown();
    }

    #[test]
    fn garbage_between_messages_is_skipped() {
        let (_fx, session) = Fixture::new("garbage");
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
        session.shutdown();
    }

    #[test]
    fn permission_request_reaches_the_hook_and_the_answer_completes_the_turn() {
        let (fx, session) = Fixture::new("permission");
        let session = Arc::new(session);
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        let ask = fx.ask();
        assert_eq!(ask.title, "rm -rf /");
        assert_eq!(ask.kind.as_deref(), Some("execute"));
        assert_eq!(ask.options.len(), 2);
        session.answer_permission(&ask.request, "allow");
        assert_eq!(worker.join().unwrap(), Ok("ok:allow".to_string()));
        assert!(fx.wait_for("perm:selected", 1));
        // Every other open window has to retire the row this one answered,
        // and draw the option that actually won rather than its own click.
        assert_eq!(fx.settled(), (ask.request, Some("allow".to_string())));
        session.shutdown();
    }

    #[test]
    fn a_timeout_before_the_answer_sends_the_cancelled_outcome() {
        let (fx, session) = Fixture::new("permission");
        let session = session.with_timeout(Duration::from_millis(700));
        let reply = session.complete(&asking("hi"));
        assert!(reply.is_err(), "{reply:?}");
        let ask = fx.ask();
        assert!(fx.wait_for("cancel", 1));
        assert!(fx.wait_for("perm:cancelled", 1));
        // A row nobody answered is retired too, or its buttons outlive the
        // turn — with no option, because nothing was chosen.
        assert_eq!(fx.settled(), (ask.request, None));
        session.shutdown();
    }

    #[test]
    fn a_slow_turn_is_cancelled_and_the_next_one_works() {
        let (fx, session) = Fixture::new("slow");
        let session = session.with_timeout(Duration::from_millis(500));
        let reply = session.complete(&asking("hi"));
        assert!(
            reply.as_ref().is_err_and(|why| why.contains("cancelled")),
            "{reply:?}"
        );
        assert!(fx.wait_for("cancel", 1));
        assert_eq!(session.complete(&asking("again")), Ok("Hello".to_string()));
        assert_eq!(fx.count("spawn"), 1, "cancel is not a respawn");
        session.shutdown();
    }

    #[test]
    fn a_child_that_exits_mid_turn_is_respawned_on_the_next_wake() {
        let (fx, session) = Fixture::new("exit");
        // The wait a death buys is the next test's subject; this one is about
        // the respawn that has to happen once the wait is over.
        let session = session.with_backoff(Duration::ZERO);
        assert_eq!(
            session.complete(&asking("hi")),
            Err("harness exited".to_string())
        );
        assert!(!session.inspect().alive);
        assert_eq!(session.complete(&asking("again")), Ok("Hello".to_string()));
        assert_eq!(fx.count("spawn"), 2);
        assert!(session.inspect().alive);
        session.shutdown();
    }

    /// #437: the count `a_missing_binary_backs_off_instead_of_respawning`
    /// exercises is shared with every other way a wake goes unserved, so a
    /// Harness that dies is not respawned on the very next wake.
    #[test]
    fn a_death_under_a_turn_buys_the_same_wait_a_failed_spawn_does() {
        let (fx, session) = Fixture::new("die");
        assert_eq!(
            session.complete(&asking("hi")),
            Err("harness exited".to_string())
        );
        let next = session.complete(&asking("again")).unwrap_err();
        assert!(next.contains("retrying in"), "{next}");
        assert_eq!(fx.count("spawn"), 1, "the dying child was spawned again");
        session.shutdown();
    }

    /// The death that reaches nobody: `initialize` is answered and the child
    /// exits before `session/new`, so the loss surfaces in `open_session`
    /// rather than in a turn. Charged all the same, or this Harness is
    /// respawned on every wake forever.
    #[test]
    fn a_death_before_the_session_opens_buys_the_same_wait() {
        let (fx, session) = Fixture::new("die-opening");
        assert_eq!(
            session.complete(&asking("hi")),
            Err("harness exited".to_string())
        );
        assert_eq!(fx.count("new"), 1);
        let next = session.complete(&asking("again")).unwrap_err();
        assert!(next.contains("retrying in"), "{next}");
        assert_eq!(fx.count("spawn"), 1, "the dying child was spawned again");
        session.shutdown();
    }

    /// The other half of one counter: a turn the child answered clears a death
    /// from earlier, whether the answer was a reply, a stop reason, or a
    /// timeout we gave up on.
    #[test]
    fn a_turn_the_child_answered_clears_an_earlier_death() {
        let (_fx, session) = Fixture::new("exit");
        let session = session.with_backoff(Duration::ZERO);
        assert_eq!(
            session.complete(&asking("hi")),
            Err("harness exited".to_string())
        );
        assert_eq!(session.complete(&asking("again")), Ok("Hello".to_string()));
        let state = session.state.lock().unwrap();
        assert_eq!(state.spawn_failures, 0, "the death is still counted");
        assert!(
            state.spawn_wait_until.is_none(),
            "the served turn left a wait behind"
        );
        drop(state);
        session.shutdown();
    }

    #[test]
    fn auth_required_names_the_login_and_the_retry_gate_holds() {
        let (fx, session) = Fixture::new("auth");
        let reply = session.complete(&asking("hi"));
        assert_eq!(reply, Err(not_authenticated("fake --login")));
        assert_eq!(session.inspect().login.as_deref(), Some("fake --login"));
        // Inside the gate: fails fast, no second session/new on the wire.
        assert!(session.complete(&asking("hi")).is_err());
        assert_eq!(fx.count("new"), 1);
        session.shutdown();

        let (fx, session) = Fixture::new("auth");
        let session = session.with_auth_retry(Duration::ZERO);
        assert!(session.complete(&asking("hi")).is_err());
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
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
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
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
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
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
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
        assert_eq!(fx.count("load"), 0);
        session.shutdown();
    }

    /// #448: `hermes` answers a `session/load` it cannot honour with an empty
    /// success result, so the id is dead and only a turn says so. One reopen,
    /// and the retry lands on a session the Harness actually has.
    #[test]
    fn a_load_that_did_not_restore_reopens_once_and_the_retry_lands() {
        let (fx, session) = Fixture::new("load-dead");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"session_id":"saved-ok","harness":"fake"}"#,
        )
        .unwrap();
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
        assert_eq!(fx.count("load"), 1);
        assert_eq!(fx.count("new"), 1, "the dead id was kept");
        assert_eq!(fx.count("prompt"), 2);
        assert_eq!(fx.count("spawn"), 1, "a reopen is not a respawn");
        assert_eq!(session.inspect().session_id.as_deref(), Some("fresh-id"));
        // And the next launch cannot read the dead id back out of the file.
        let saved = std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap();
        assert!(saved.contains("fresh-id"), "{saved}");
        session.shutdown();
    }

    /// The other half of #448: the reopen is one attempt, not a ladder. A
    /// Harness that refuses the fresh session too has answered.
    #[test]
    fn a_reopened_session_that_refuses_again_is_a_refusal() {
        let (fx, session) = Fixture::new("load-refusal");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"session_id":"saved-ok","harness":"fake"}"#,
        )
        .unwrap();
        let reply = session.complete(&asking("hi"));
        assert!(
            reply.as_ref().is_err_and(|why| why.contains("refusal")),
            "{reply:?}"
        );
        assert_eq!(fx.count("new"), 1);
        assert_eq!(fx.count("prompt"), 2, "the reopen was tried more than once");
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
        let first = session.complete(&asking("hi")).unwrap_err();
        assert!(first.contains("could not start"), "{first}");
        let second = session.complete(&asking("hi")).unwrap_err();
        assert!(second.contains("retrying in"), "{second}");
        assert_eq!(session.backoff(1), BACKOFF_FIRST);
        assert_eq!(session.backoff(2), Duration::from_secs(10));
        assert_eq!(session.backoff(40), BACKOFF_CAP);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// ADR-0016's newest-wins, at the Harness seam: the Poke that arrives
    /// under a turn takes it rather than being refused (#472).
    #[test]
    fn a_newer_wake_cancels_the_turn_in_flight_and_takes_it() {
        let (fx, session) = Fixture::new("slow");
        let session = Arc::new(session.with_timeout(Duration::from_secs(10)));
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        // The first prompt is on the wire, so the turn lock is held and the
        // wake below is the one that has to displace it.
        assert!(fx.wait_for("prompt", 1), "the first turn never went out");
        assert_eq!(session.complete(&asking("again")), Ok("Hello".to_string()));
        assert_eq!(
            fx.count("cancel"),
            1,
            "the displaced turn was not cancelled"
        );
        assert_eq!(fx.count("prompt"), 2, "the second prompt never went out");
        // A withdrawal, not a failure: cancelled by name in the log, no
        // `refused` line, and no respawn backoff charged (#437).
        let displaced = worker.join().unwrap().unwrap_err();
        assert!(displaced.contains("cancelled"), "{displaced}");
        assert!(
            fx.events("refused").is_empty(),
            "{:?}",
            fx.events("refused")
        );
        assert_eq!(session.state.lock().unwrap().spawn_failures, 0);
        let turns = fx.events("turn");
        assert_eq!(turns[0]["stop"], json!("cancelled"), "{turns:?}");
        session.shutdown();
    }

    /// The one wake that waits instead: an ambient tick cancelling the Poke it
    /// arrived behind would be worse than the refusal (#472).
    #[test]
    fn a_proactive_wake_never_cancels_a_reactive_turn() {
        let (fx, session) = Fixture::new("slow");
        let session = Arc::new(session.with_timeout(Duration::from_secs(3)));
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        assert!(fx.wait_for("prompt", 1), "the first turn never went out");
        assert_eq!(
            session.complete(&ambient("again")),
            Err("harness busy".to_string())
        );
        // Read before the reactive turn's own timeout cancel, which is the
        // only other thing that would put a `cancel` on this wire.
        assert_eq!(fx.count("cancel"), 0, "the Poke was cancelled for a tick");
        let _ = worker.join();
        session.shutdown();

        // #435: the refused wake sent no prompt, so without a line of its own
        // the `parsed` line the Shell writes for it would read against the
        // prompt the wake before it logged.
        let refused = fx.events("refused");
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0]["instance"], json!("buddy-1"));
        assert_eq!(refused[0]["wake"], json!("proactive"));
        assert_eq!(refused[0]["why"], json!("harness busy"));
        assert_eq!(fx.events("prompt").len(), 1, "the refused wake sent none");
    }

    /// The probe's two exit codes are its two phases: 1 is a turn that did
    /// not finish, whatever the reply text turns out to be, and 2 is never
    /// having asked.
    #[test]
    fn the_probe_exits_zero_on_a_turn_one_on_a_refusal_and_two_on_no_attach() {
        let (_fx, session) = Fixture::new("happy");
        assert_eq!(probe(&session), 0);
        session.shutdown();

        let (_fx, session) = Fixture::new("refusal");
        assert_eq!(probe(&session), 1);
        session.shutdown();

        let (_fx, session) = Fixture::new("auth");
        assert_eq!(probe(&session), 2);
        session.shutdown();

        let dir = std::env::temp_dir().join(format!("ai-buddy-probe-{}", uuid::Uuid::new_v4()));
        let launch = Launch {
            name: "nope".into(),
            argv: vec!["/nonexistent/ai-buddy-no-such-harness".into()],
        };
        assert_eq!(
            probe(&Session::new(launch, dir.clone(), Box::new(|_| {}))),
            2
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// `shutdown` kills and waits; a zero-length wait can only succeed if
    /// that wait already finished.
    #[test]
    fn the_probe_waits_for_the_child_to_be_reaped() {
        let (_fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok("Hello".to_string()));
        let wire = session.current_wire().expect("attached");
        session.shutdown();
        assert!(
            wire.wait_for_exit(Duration::ZERO),
            "the wire thread outlived the reap"
        );
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

    fn mcp_tmp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ai-buddy-mcp-launch-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        std::fs::write(path, []).unwrap();
    }

    fn sidecar(dir: &Path) -> PathBuf {
        dir.join(if cfg!(windows) {
            "ai-buddy-mcp.exe"
        } else {
            "ai-buddy-mcp"
        })
    }

    #[test]
    fn mcp_launch_prefers_an_env_file_over_a_sibling() {
        let dir = mcp_tmp("env-wins");
        let env_bin = dir.join("from-env");
        let current_exe = dir.join("ai-buddy");
        touch(&env_bin);
        touch(&sidecar(&dir));
        touch(&current_exe);

        let launch = mcp_launch(Some(env_bin.as_path()), &current_exe).expect("env file wins");
        assert_eq!(launch.path, env_bin);
        assert!(launch.args.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcp_launch_falls_through_to_a_sibling_when_env_is_missing() {
        let dir = mcp_tmp("sibling");
        let current_exe = dir.join("ai-buddy");
        let sibling = sidecar(&dir);
        touch(&sibling);
        touch(&current_exe);

        let launch = mcp_launch(None, &current_exe).expect("sibling");
        assert_eq!(launch.path, sibling);
        assert!(launch.args.is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcp_launch_runs_the_app_binary_as_stdio_when_nothing_is_beside_it() {
        let dir = mcp_tmp("stdio");
        let current_exe = dir.join("ai-buddy");
        touch(&current_exe);

        let launch = mcp_launch(None, &current_exe).expect("app binary");
        assert_eq!(launch.path, current_exe);
        assert_eq!(launch.args, ["--mcp-stdio"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcp_launch_returns_none_when_the_exe_is_not_the_app() {
        let dir = mcp_tmp("test-bin");
        let current_exe = dir.join("harness-unit-tests");
        touch(&current_exe);

        assert_eq!(mcp_launch(None, &current_exe), None);

        let _ = std::fs::remove_dir_all(dir);
    }
}
