//! The attached Harness as the Completer.
//! Spawns the Harness in ACP mode; every wake is one `session/prompt`
//! (ADR-0008, ADR-0018). Auth is the Harness's own. Protocol in `acp_wire.rs`.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use ai_buddy_core::director::{Completer, Reply, Wake, WakeRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::acp_wire::{
    Event, Handshake, McpChoice, McpLaunch, OpenError, SpawnError, TurnError, Wire,
};
use crate::action_log;

pub use crate::acp_wire::{ElicitationAnswer, ElicitationForm, PermissionAsk, PlanStep};

/// `pub(crate)` so the settings window can name the variable that owns a row.
pub(crate) const VAR: &str = "AI_BUDDY_HARNESS";
/// Where the stdio MCP server binary is, when it is not beside the app.
pub(crate) const MCP_BIN: &str = "AI_BUDDY_MCP_BIN";
/// Spawn `current_dir` and ACP session cwd. Empty is the data folder, not
/// `$HOME`: bare home mixes app files with harness project configs (#782).
pub(crate) const CWD: &str = "AI_BUDDY_HARNESS_CWD";
/// How long an unauthenticated Harness is left alone, in seconds. Named here
/// so the Development row it owns can print it.
pub(crate) const AUTH_RETRY_SECS: &str = "AI_BUDDY_HARNESS_AUTH_RETRY_SECS";
/// How long a Harness `session/prompt` may run, in seconds. Named here so
/// the Development row it owns can print it.
pub(crate) const TURN_TIMEOUT_SECS: &str = "AI_BUDDY_HARNESS_TURN_TIMEOUT";

/// The one file the session survives a restart in.
const SESSION_FILE: &str = "harness-session.json";

/// How long a not-yet-authenticated Harness is left alone before `session/new`
/// is tried again. Long enough not to hammer it, short enough that a user who
/// runs the login command sees the buddy pick it up without a restart.
const AUTH_RETRY: Duration = Duration::from_secs(60);

/// What an empty auth-retry field means, in seconds.
pub(crate) fn auth_retry_placeholder() -> String {
    AUTH_RETRY.as_secs().to_string()
}

/// How long a Harness `session/prompt` may run before `session/cancel`.
/// Twenty seconds cancelled a web lookup. Forever leaves a hung child.
/// Two minutes covers a lookup and still maps expiry to cancel (ADR-0017).
pub(crate) const TURN_TIMEOUT: Duration = Duration::from_secs(120);

/// Settings / `AI_BUDDY_HARNESS_TURN_TIMEOUT` still wins when set.
pub(crate) fn turn_timeout() -> Duration {
    crate::dev_flags::harness_turn_timeout_secs().map_or(TURN_TIMEOUT, Duration::from_secs)
}

pub(crate) fn turn_timeout_placeholder() -> String {
    TURN_TIMEOUT.as_secs().to_string()
}

/// Respawn backoff after a wake the child could not serve. Doubles from the
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

/// How long a withdrawal waits for the `parsed` line that belongs to it. The
/// Shell writes that line frames after the turn ended, so this only has to
/// outlast one frame. Being wrong mislabels one wake and does not leak.
const WITHDRAWAL_GRACE: Duration = Duration::from_secs(30);

/// The stop reason a cancelled turn comes back with. A turn the Completer gave
/// up waiting on is `TurnError::Timeout` instead, so this reason on a turn is
/// always a cancel someone else asked for.
const CANCELLED: &str = "cancelled";

/// Which Harness, and the command line that starts it in ACP mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub name: String,
    pub argv: Vec<String>,
}

/// The launch table. `None` is the HTTP Completer path. Unknown values are a
/// command line of the user's own. Antigravity (`agy`) does not speak ACP, so
/// Google has no row. Keep the README Harness Support table in step.
pub fn launch(value: Option<&str>) -> Option<Launch> {
    let value = value?.trim();
    let (name, argv): (&str, Vec<&str>) = match value {
        "" => return None,
        // `@latest` is load-bearing. npx serves the first cache it built, and
        // the adapter bundles the Claude Code it was built against. A pin
        // drifts the same way, slower. `claude update` hits a different install.
        "claude" => (
            value,
            vec!["npx", "-y", "@agentclientprotocol/claude-agent-acp@latest"],
        ),
        "codex" => (
            value,
            vec!["npx", "-y", "@agentclientprotocol/codex-acp@latest"],
        ),
        // `acp` is absent from `cursor-agent --help`, which lists `agent`,
        // `login` and `mcp`. It answers `initialize` all the same.
        "cursor-agent" => (value, vec!["cursor-agent", "acp"]),
        // `grok` alone is the interactive TUI. The ACP agent is the
        // subcommand. Without this arm the escape hatch below launches that
        // TUI on stdio and the attach times out.
        "grok" => (value, vec!["grok", "agent", "stdio"]),
        // `goose` alone is the interactive CLI. ACP over stdio is the `acp`
        // subcommand; the ACP registry publishes the same argv.
        "goose" => (value, vec!["goose", "acp"]),
        "hermes" => (value, vec!["hermes", "acp"]),
        "opencode" => (value, vec!["opencode", "acp"]),
        "pi" => (value, vec!["npx", "-y", "pi-acp@latest"]),
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
/// Exported-and-empty is not unexported. `AI_BUDDY_HARNESS=` is the kill
/// switch, and falling through would spawn the Harness the export cleared.
pub fn from_settings(saved: Option<&str>) -> Option<Launch> {
    match std::env::var(VAR) {
        Ok(exported) => launch(Some(&exported)),
        Err(_) => launch(saved),
    }
}

impl Launch {
    /// The child, inheriting our environment untouched. ADR-0010. No provider
    /// key, no `CLAUDE_CONFIG_DIR`, no `--bare`. Own process group once
    /// `own_interrupt` has taken Ctrl+C, so a SIGINT on `cargo run` misses it.
    fn command(&self, cwd: &AttachCwd) -> Command {
        let mut command = Command::new(&self.argv[0]);
        command.args(&self.argv[1..]).current_dir(cwd.as_path());
        isolate_from_interrupt(&mut command);
        command
    }

    fn line(&self) -> String {
        self.argv.join(" ")
    }
}

/// Spawn `current_dir` and ACP session cwd. User-owned. Never `create_dir_all`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AttachCwd(PathBuf);

/// Directory we own. `create_dir_all` OK. Session file and Action Log.
struct SessionDataDir(PathBuf);

/// Retarget identity. Launch alone would Stand on a cwd-only edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    launch: Launch,
    cwd: Result<AttachCwd, CwdError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CwdError {
    Relative(PathBuf),
}

impl fmt::Display for CwdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CwdError::Relative(path) => {
                write!(f, "relative path {} is not a project", path.display())
            }
        }
    }
}

fn attach_cwd_display(cwd: &Result<AttachCwd, CwdError>) -> String {
    match cwd {
        Ok(cwd) => cwd.as_path().display().to_string(),
        Err(error) => error.to_string(),
    }
}

/// Where the Harness runs when the Working directory row is blank, for that
/// row's placeholder. Resolved rather than described, so the row shows the
/// path instead of leaving the user to know it (#913). Reads
/// `AI_BUDDY_HARNESS_CWD` first, because a row the environment owns runs
/// somewhere else again.
pub(crate) fn attach_cwd_placeholder() -> String {
    attach_cwd_display(&AttachCwd::resolve(&crate::model::env_or_file(CWD, "")))
}

impl AttachCwd {
    fn resolve(raw: &str) -> Result<Self, CwdError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(Self(ai_buddy_core::memory::data_dir()));
        }
        let path = PathBuf::from(trimmed);
        if !path.is_absolute() {
            return Err(CwdError::Relative(path));
        }
        Ok(Self(path))
    }

    fn as_path(&self) -> &Path {
        &self.0
    }

    /// Existing directory, else spawn names this path. Do not create it.
    fn checked(&self) -> Result<(), SpawnError> {
        if self.0.is_dir() {
            Ok(())
        } else {
            Err(SpawnError::Failed(self.0.display().to_string()))
        }
    }
}

impl SessionDataDir {
    fn app() -> Self {
        Self(ai_buddy_core::memory::data_dir())
    }

    fn probe() -> Self {
        Self(ai_buddy_core::memory::data_dir().join("probe"))
    }

    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        Self(path)
    }

    fn ensure(&self) -> Result<(), SpawnError> {
        std::fs::create_dir_all(&self.0)
            .map_err(|why| SpawnError::Failed(format!("{}: {why}", self.0.display())))
    }

    fn as_path(&self) -> &Path {
        &self.0
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Target {
    pub(crate) fn from_settings(source: Option<&str>, cwd_row: &str) -> Option<Self> {
        from_settings(source).map(|launch| Self {
            launch,
            cwd: AttachCwd::resolve(&crate::model::env_or_file(CWD, cwd_row)),
        })
    }
}

/// Tests isolate without installing a `ctrlc` handler. Production stays
/// false until `own_interrupt`. A failed handler must not orphan a tree
/// Ctrl+C can no longer reach.
static INTERRUPT_OWNED: AtomicBool = AtomicBool::new(cfg!(test));
static INTERRUPT_QUITTING: AtomicBool = AtomicBool::new(false);

/// Isolation is on. Ctrl+C is ours, so the child may leave this process group.
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

#[cfg(windows)]
pub(crate) fn get_creation_flags() -> u32 {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    CREATE_NEW_PROCESS_GROUP
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
        // The Job Object is created and assigned at spawn time in
        // `acp_wire::windows_job::spawn_in_job`, so descendants die on
        // shutdown. Still a new process group, so Ctrl+C does not reach the child.
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
    /// Attached but not authenticated. The one command that fixes it.
    pub login: Option<String>,
    /// Whether `initialize` offered HTTP MCP.
    pub mcp_http: bool,
    pub alive: bool,
    /// The binary `PATH` has not got, when that is why nothing is running.
    /// Told apart from a child that died because no respawn mends it and the
    /// sentence a user needs is a different one.
    pub missing: Option<String>,
    /// Whether ACP handshake/spawn is in progress. Gates chat until ready or failed.
    pub initializing: bool,
    /// What the last turn came back with, when it came back with an error, and
    /// `None` once a turn answers. A Harness that refuses every prompt is
    /// attached, alive, and authenticated, so nothing else here tells it apart.
    pub last_error: Option<String>,
}

/// One remembered ACP session, keyed so a restart can load it for that
/// identity and no other.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct SavedSlot {
    instance: String,
    character: String,
    /// Blank-AI mode's slot, kept apart from the shaped one. Defaulted so a
    /// file written before the mode existed loads as a session opened with a
    /// Character Prompt in it.
    #[serde(default)]
    blank: bool,
    session_id: String,
}

/// What `harness-session.json` holds. `session_id` is the older single
/// pointer, kept so an existing file still parses. That leftover is not
/// applied to any Character Instance.
#[derive(Deserialize, Serialize)]
struct SavedSession {
    harness: String,
    agent: Option<String>,
    #[serde(default)]
    sessions: Vec<SavedSlot>,
    #[serde(default, skip_serializing)]
    session_id: Option<String>,
}

impl SavedSlot {
    /// The identity half of a remembered slot. One place, so the three sites
    /// that match a slot against a key cannot drift apart when the key gains
    /// a field.
    fn key(&self) -> SessionKey {
        SessionKey {
            instance: self.instance.clone(),
            character: self.character.clone(),
            blank: self.blank,
        }
    }
}

/// Instance plus Character. Two Instances of one Character do not share, and
/// a retarget is a different Character Prompt (ADR-0012). Blank-AI mode joins
/// them so a shaped session cannot keep answering after the mode switches on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SessionKey {
    instance: String,
    character: String,
    blank: bool,
}

impl SessionKey {
    fn from_request(request: &WakeRequest) -> Self {
        Self {
            instance: request.instance.clone(),
            character: request.character.clone(),
            blank: request.blank,
        }
    }
}

/// What the session on the wire tells the Chat surface, live.
/// `Settled` exists because an ask goes to every open surface and only one
/// takes the click. Without it the others keep offering live buttons.
#[derive(Debug)]
pub enum Forwarded {
    Ask(PermissionAsk),
    Form(ElicitationForm),
    /// A request that is no longer answerable, and the option that won it.
    /// `None` when nothing was picked and the turn simply ended.
    Settled {
        request: String,
        option: Option<String>,
    },
    /// The Harness's thinking so far, blank lines included. The strip
    /// scrolls inside a fixed box; this is the whole thought. ADR-0025.
    Thought(String),
    /// The agent's plan, replacing whatever the surface holds. Empty ends it.
    Plan(Vec<PlanStep>),
    /// Preflight finished. Chat's first ReloadChat races this thread, so a
    /// missing launcher would otherwise never reach the landing (#726).
    AttachSettled,
}

type Forward = Box<dyn Fn(Forwarded) + Send + Sync>;

/// The attached Harness. One child, one ACP connection, and one conversation
/// per Character Instance identity.
pub struct Session {
    launch: Launch,
    cwd: Result<AttachCwd, CwdError>,
    data: SessionDataDir,
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
    /// Instance whose wake holds `turn`. One child serves every Instance
    /// (ADR-0008) and `wire.cancel` names no session, so a cancel without this
    /// name lands on whichever buddy is mid-reply.
    serving_instance: Mutex<Option<String>>,
    /// The Instance a cancel has just gone out for, until the turn it cancels
    /// names itself. One slot, because one prompt is in flight at a time.
    withdrawing: Mutex<Option<String>>,
    /// Withdrawn turn to the Instance whose wake took the session, until the
    /// `parsed` line for that wake. Keyed by loser so a later cancel cannot
    /// pair a withdrawn `turn` line with a `parsed` line that says failed.
    withdrawn: Mutex<HashMap<String, (String, Instant)>>,
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

struct OpenedSession {
    id: String,
    /// Whether this id came from `session/load` and has not served a turn
    /// yet. That is the one condition `reopen_loaded` answers to.
    loaded: bool,
}

#[derive(Default)]
struct State {
    sessions: HashMap<SessionKey, OpenedSession>,
    handshake: Handshake,
    login: Option<String>,
    auth_tried: Option<Instant>,
    spawn_failures: u32,
    spawn_wait_until: Option<Instant>,
}

impl State {
    /// The child answered, so no earlier death is a death in a row any more.
    /// Every outcome but a loss proves it. A stop reason, and even a turn we
    /// gave up waiting on, came back from a process that is still there.
    fn answered(&mut self) {
        self.spawn_failures = 0;
        self.spawn_wait_until = None;
    }
}

/// Names the Instance whose wake holds the turn lock, for exactly as long as
/// it holds it. A save reads that name before cancelling. A leftover slot
/// would aim the cancel at the turn that replaced it.
struct Serving<'a>(&'a Session);

impl<'a> Serving<'a> {
    fn new(session: &'a Session, instance: &str) -> Self {
        if let Ok(mut slot) = session.serving_instance.lock() {
            *slot = Some(instance.to_string());
        }
        Self(session)
    }
}

impl Drop for Serving<'_> {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.serving_instance.lock() {
            *slot = None;
        }
    }
}

impl Session {
    fn new(
        launch: Launch,
        cwd: Result<AttachCwd, CwdError>,
        data: SessionDataDir,
        forward: Arc<Forward>,
    ) -> Self {
        let inspect = HarnessInspect {
            name: launch.name.clone(),
            command: launch.line(),
            ..Default::default()
        };
        Self {
            launch,
            cwd,
            data,
            forward,
            timeout: turn_timeout(),
            auth_retry: crate::dev_flags::harness_auth_retry_secs()
                .map_or(AUTH_RETRY, Duration::from_secs),
            backoff_first: BACKOFF_FIRST,
            turn: Mutex::new(()),
            serving_reactive: AtomicBool::new(false),
            serving_instance: Mutex::new(None),
            withdrawing: Mutex::new(None),
            withdrawn: Mutex::new(HashMap::new()),
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

    fn target(&self) -> Target {
        Target {
            launch: self.launch.clone(),
            cwd: self.cwd.clone(),
        }
    }

    fn backoff(&self, failures: u32) -> Duration {
        self.backoff_first
            .saturating_mul(1u32 << failures.saturating_sub(1).min(16))
            .min(BACKOFF_CAP)
    }

    /// One counter and one offset for every way a wake goes unserved. A spawn
    /// that never started, an `initialize` that never came back, and a child
    /// that died under a turn must not price the same failure twice.
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

    /// `initialize` and `session` methods get at least ten seconds whatever
    /// the turn timeout is. `npx` starting Zed's adapter cold is slower than any reply.
    fn attach_timeout(&self) -> Duration {
        self.timeout.max(Duration::from_secs(10))
    }

    /// Where the Harness should be by the time the first wake arrives. Spawned
    /// so startup does not wait on `npx`; the outcome is one stderr line.
    pub fn spawn_preflight(self: &Arc<Self>) {
        self.update_inspect(|inspect| inspect.initializing = true);
        let session = Arc::clone(self);
        thread::spawn(move || {
            let attached = session.attach(None);
            match &attached {
                Ok(_) => eprintln!("harness: {} attached", session.launch.name),
                Err(why) => {
                    eprintln!("harness: {why}; StaticDirector is in force until it answers")
                }
            }
            if attached.is_err() {
                session.update_inspect(|inspect| inspect.initializing = false);
            }
            // Chat's ReloadChat after a pick races this thread. A second
            // opening is how `inspect.missing` reaches the landing (#726).
            (session.forward)(Forwarded::AttachSettled);
        });
    }

    /// Take the turn lock from the wake in flight by cancelling it, or `None`
    /// for the one wake that may not. Newest-wins (ADR-0016) on one session
    /// (ADR-0008). A reactive Poke must not give way to the next ambient tick.
    fn supersede(&self, request: &WakeRequest) -> Option<MutexGuard<'_, ()>> {
        if !request.reactive && self.serving_reactive.load(Ordering::SeqCst) {
            return None;
        }
        let wire = self.current_wire()?;
        self.note_withdrawal(Some(request.instance.clone()));
        wire.cancel();
        let until = Instant::now() + HANDOVER;
        loop {
            if let Ok(turn) = self.turn.try_lock() {
                return Some(turn);
            }
            if Instant::now() >= until {
                // A Harness that ignored the cancel keeps its turn, so no turn
                // was withdrawn for this wake and the next cancelled one must
                // not be read as though it were.
                self.note_withdrawal(None);
                return None;
            }
            thread::sleep(HANDOVER_POLL);
        }
    }

    /// Say that the turn in flight is being taken for `winner`'s wake, or that
    /// no turn was taken after all. Written before the cancel goes out. The
    /// loser holds the lock until it has written its log line.
    fn note_withdrawal(&self, winner: Option<String>) {
        if let Ok(mut slot) = self.withdrawing.lock() {
            *slot = winner;
        }
    }

    /// The Instance a turn of `loser`'s was withdrawn for, named by the turn
    /// itself and kept for the `parsed` line the Shell writes for that wake.
    fn claim_withdrawn_turn(&self, loser: &str, reason: &str) -> Option<String> {
        if reason != CANCELLED {
            return None;
        }
        let winner = self.withdrawing.lock().ok()?.take()?;
        if let Ok(mut withdrawn) = self.withdrawn.lock() {
            withdrawn.insert(loser.to_string(), (winner.clone(), Instant::now()));
        }
        Some(winner)
    }

    /// The same withdrawal, taken by the wake that lost the session. Taken
    /// rather than read so it cannot colour this Instance's next wake. The
    /// grace bounds the entry no `parsed` line comes for (ADR-0016).
    fn claim_withdrawn_wake(&self, instance: &str) -> Option<String> {
        let (winner, at) = self.withdrawn.lock().ok()?.remove(instance)?;
        (at.elapsed() < WITHDRAWAL_GRACE).then_some(winner)
    }

    fn turn(&self, request: &WakeRequest) -> Result<Reply, String> {
        let _turn = match self.turn.try_lock() {
            Ok(turn) => turn,
            Err(_) => match self.supersede(request) {
                Some(turn) => turn,
                None => return Err(self.refused(request, "harness busy")),
            },
        };
        self.serving_reactive
            .store(request.reactive, Ordering::SeqCst);
        // Declared after the turn lock, so it is cleared before the lock is
        // released. No window has the lock free and this slot still naming a
        // turn that has ended.
        let _serving = Serving::new(self, &request.instance);
        let (session_id, outcome) = self.attempt(request)?;
        // `session/load` can succeed with a dead id (`hermes`). The first turn
        // is the evidence. Reopen once. Not a loss (already charged), a
        // timeout (would spend the budget twice), or our own cancel (ADR-0012).
        let key = SessionKey::from_request(request);
        let (session_id, outcome) = match &outcome {
            Err(TurnError::Stopped(reason)) if reason != CANCELLED && self.reopen_loaded(&key) => {
                self.attempt(request)?
            }
            Err(TurnError::Failed(_)) if self.reopen_loaded(&key) => self.attempt(request)?,
            _ => (session_id, outcome),
        };
        let mut withdrawn = false;
        let answer = match outcome {
            Ok(reply) => {
                // A cap-ended turn is logged as what was shown and why there
                // was no more of it, the same pair the HTTP lane writes.
                action_log::append(
                    self.data.as_path(),
                    "turn",
                    match reply.truncated {
                        true => json!({"text": reply.text, "stop": "max_tokens"}),
                        false => json!({"text": reply.text}),
                    },
                );
                Ok(reply)
            }
            Err(TurnError::Lost) => Err(LOST.to_string()),
            Err(TurnError::Timeout) => {
                action_log::append(
                    self.data.as_path(),
                    "timeout",
                    json!({"session_id": session_id}),
                );
                Err(format!(
                    "harness turn exceeded {}s; cancelled",
                    self.timeout.as_secs()
                ))
            }
            Err(TurnError::Stopped(reason)) => {
                // One child serves every Instance (ADR-0008), so a cancel for
                // another buddy's wake reaches this turn without superseding
                // this Instance's slot. The log line must say it was given up.
                let withdrawn_for = self.claim_withdrawn_turn(&request.instance, &reason);
                withdrawn = withdrawn_for.is_some();
                action_log::append(
                    self.data.as_path(),
                    "turn",
                    json!({"stop": reason, "withdrawn_for": withdrawn_for}),
                );
                Err(format!("harness stopped: {reason}"))
            }
            Err(TurnError::Busy) => Err("harness busy".to_string()),
            Err(TurnError::Failed(why)) => {
                action_log::append(self.data.as_path(), "turn", json!({"error": why}));
                Err(format!("harness: {why}"))
            }
        };
        // Kept for the readers on the other side of the Completer, where
        // `Result<String, String>` narrows to "no proposal". A withdrawal is
        // not among them. Naming our own cancel would report a Harness fault.
        self.update_inspect(|inspect| {
            inspect.last_error = (!withdrawn)
                .then(|| answer.as_ref().err().cloned())
                .flatten();
        });
        answer
    }

    /// One `session/prompt` on the session `attach` hands over. Split out of
    /// `turn` so the load retry pays the same bookkeeping the first attempt
    /// did. `Err` is a wake that never reached the wire, already refused.
    fn attempt(&self, request: &WakeRequest) -> Result<(String, Result<Reply, TurnError>), String> {
        let (wire, session_id) = self
            .attach(Some(&SessionKey::from_request(request)))
            .map_err(|why| self.refused(request, &why))?;
        // The Instance and the wake kind come through the seam rather than
        // from anything here. One child serves every buddy, so the process is
        // not whose wake this is.
        action_log::append(
            self.data.as_path(),
            "prompt",
            json!({
                "session_id": session_id,
                "instance": request.instance,
                "wake": wake_kind(request.reactive),
                "chars": request.prompt.len(),
            }),
        );
        let outcome = wire.prompt(&session_id, &request.prompt, self.timeout);
        // Charge the loss before the outcome is dressed for the caller. A
        // death under every turn must pay the same backoff a failed spawn
        // does. An answering child must not carry a death from hours ago.
        if let Ok(mut state) = self.state.lock() {
            match &outcome {
                Err(TurnError::Lost) => self.lost(&wire, &mut state),
                _ => state.answered(),
            }
            // A finished turn is the proof `session/load` was not, so any
            // later failure on this session is the Harness's own answer.
            if outcome.is_ok() {
                if let Some(opened) = state.sessions.get_mut(&SessionKey::from_request(request)) {
                    opened.loaded = false;
                }
            }
        }
        Ok((session_id, outcome))
    }

    /// Drop a loaded session so the next `attach` opens a fresh one. A refused
    /// `session/new` is the Harness answering, and reopening it would loop.
    /// The file goes first so a restart cannot load the same id back.
    fn reopen_loaded(&self, key: &SessionKey) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let Some(opened) = state.sessions.get(key) else {
            return false;
        };
        if !opened.loaded {
            return false;
        }
        let dropped = opened.id.clone();
        state.sessions.remove(key);
        self.drop_saved(key);
        self.update_inspect(|inspect| {
            if inspect.session_id.as_deref() == Some(dropped.as_str()) {
                inspect.session_id = None;
            }
        });
        true
    }

    /// A wake that never reached `session/prompt`. Logged because the Shell
    /// writes a `parsed` line for every reply it takes, this refusal included.
    /// A `parsed` with no line of its own would join the last logged wake.
    fn refused(&self, request: &WakeRequest, why: &str) -> String {
        action_log::append(
            self.data.as_path(),
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
    /// Empty the slot here. `alive()` flips only once the wire's thread has
    /// dropped its receiver, and a wake arriving before then would reuse it.
    fn lost(&self, wire: &Wire, state: &mut State) {
        wire.shutdown();
        if let Ok(mut slot) = self.wire.lock() {
            *slot = None;
        }
        self.update_inspect(|inspect| {
            inspect.alive = false;
            inspect.initializing = false;
        });
        self.charge_loss(state);
    }

    /// Record that the binary we spawn is not on `PATH`. The name is
    /// `argv[0]`, never the preset. `codex` attaches through `npx` and logs
    /// in through `codex`, so naming the preset would accuse the wrong binary.
    fn note_missing(&self) -> String {
        let command = self.launch.argv[0].clone();
        self.update_inspect(|inspect| {
            inspect.alive = false;
            inspect.missing = Some(command.clone());
            inspect.initializing = false;
        });
        not_installed(&command)
    }

    /// The user's answer to a forwarded permission request. Never chosen here.
    pub fn answer_permission(&self, request: &str, option: &str) {
        let Some(wire) = self.current_wire() else {
            return;
        };
        action_log::append(
            self.data.as_path(),
            "permission_answer",
            json!({"request": request, "option": option}),
        );
        wire.answer(request, option);
    }

    /// The user's answer to a forwarded elicitation form. Decline is valid.
    pub fn answer_elicitation(&self, request: &str, answer: ElicitationAnswer) {
        let Some(wire) = self.current_wire() else {
            return;
        };
        let logged = match &answer {
            ElicitationAnswer::Accept(value) => json!({"request": request, "value": value}),
            ElicitationAnswer::Decline => json!({"request": request, "action": "decline"}),
        };
        action_log::append(self.data.as_path(), "elicitation_answer", logged);
        wire.answer_elicitation(request, answer);
    }

    /// Cancel in-flight work, kill the child, and wait until it is reaped.
    /// The child is in its own process group, so ending this process does not
    /// take it with us. `npx` does not reliably die on stdin EOF.
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

    fn attach(&self, key: Option<&SessionKey>) -> Result<(Arc<Wire>, String), String> {
        if !self.wanted.load(Ordering::SeqCst) {
            return Err("harness detached".to_string());
        }
        let mut state = self.state.lock().map_err(|_| "harness state poisoned")?;
        let wire = match self.current_wire() {
            Some(wire) => wire,
            None => {
                // A new child does not have the previous process's sessions.
                state.sessions.clear();
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
                    // A file `PATH` has not got is not a child that might come
                    // back. Backoff would only refuse the next wake for up to
                    // five minutes after the user installs the CLI.
                    Err(SpawnError::Missing) => return Err(self.note_missing()),
                    Err(SpawnError::Failed(why)) => {
                        self.charge_loss(&mut state);
                        return Err(why);
                    }
                }
            }
        };
        let Some(key) = key else {
            return Ok((wire, String::new()));
        };
        if let Some(opened) = state.sessions.get(key) {
            return Ok((wire, opened.id.clone()));
        }
        if let (Some(command), Some(tried)) = (&state.login, state.auth_tried) {
            if tried.elapsed() < self.auth_retry {
                return Err(not_authenticated(command));
            }
        }
        let id = self.open_session(&wire, &mut state, key)?;
        Ok((wire, id))
    }

    fn spawn_and_initialize(&self, state: &mut State) -> Result<Arc<Wire>, SpawnError> {
        // The store first. Empty cwd is `data_dir`, which we own; `checked`
        // would refuse a first-run folder that does not exist yet. A user
        // path still goes through `checked` with no `create_dir_all` (#782).
        self.data.ensure()?;
        let cwd = match &self.cwd {
            Ok(cwd) => {
                cwd.checked()?;
                cwd
            }
            Err(error) => return Err(SpawnError::Failed(error.to_string())),
        };
        let data = self.data.as_path().to_path_buf();
        let forward = Arc::clone(&self.forward);
        let spawned = Wire::spawn(
            self.launch.command(cwd),
            self.attach_timeout(),
            Box::new(move |event| note_event(&data, &forward, event)),
        );
        // Anything but `Missing` means `PATH` had the file to run. Clear the
        // old `missing` on the failing edge too, or Settings keeps telling the
        // user to install a CLI that is already there.
        if !matches!(spawned, Err(SpawnError::Missing)) {
            self.update_inspect(|inspect| inspect.missing = None);
        }
        let wire = spawned.map_err(|why| match why {
            SpawnError::Missing => SpawnError::Missing,
            SpawnError::Failed(why) => {
                SpawnError::Failed(format!("`{}` {why}", self.launch.line()))
            }
        })?;
        state.handshake = wire.handshake().clone();
        // A fresh process is a fresh chance to sign in. The gate belonged to
        // the one that died.
        state.login = None;
        state.auth_tried = None;
        self.update_inspect(|inspect| {
            inspect.agent = state.handshake.agent.clone();
            inspect.mcp_http = state.handshake.mcp_http;
            inspect.alive = true;
            inspect.initializing = false;
        });
        let wire = Arc::new(wire);
        if !self.wanted.load(Ordering::SeqCst) {
            wire.shutdown();
            return Err(SpawnError::Failed("harness detached".to_string()));
        }
        if let Ok(mut slot) = self.wire.lock() {
            *slot = Some(Arc::clone(&wire));
        }
        if !self.wanted.load(Ordering::SeqCst) {
            self.shutdown();
            return Err(SpawnError::Failed("harness detached".to_string()));
        }
        Ok(wire)
    }

    /// `session/load` when the Harness can and the file names this Harness,
    /// else `session/new`. Either way the file ends up naming what is open.
    fn open_session(
        &self,
        wire: &Arc<Wire>,
        state: &mut State,
        key: &SessionKey,
    ) -> Result<String, String> {
        let saved = state
            .handshake
            .load_session
            .then(|| self.saved_id(key))
            .flatten();
        let mcp = mcp_server(&state.handshake);
        let cwd = self
            .cwd
            .as_ref()
            .map_err(|error| error.to_string())?
            .as_path();
        let id = match wire.open(saved.clone(), cwd, mcp.clone(), self.attach_timeout()) {
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
        state.sessions.insert(
            key.clone(),
            OpenedSession {
                id: id.clone(),
                loaded: saved.as_deref() == Some(id.as_str()),
            },
        );
        state.login = None;
        self.update_inspect(|inspect| {
            inspect.login = None;
            inspect.session_id = Some(id.clone());
        });
        self.save_session(key, &id);
        action_log::append(
            self.data.as_path(),
            "attach",
            // The label, never the choice. An `McpChoice::Http` carries the
            // loopback token and the Action Log is a file on disk.
            json!({
                "harness": self.launch.name,
                "session_id": id,
                "mcp": mcp.as_ref().map(McpChoice::label),
            }),
        );
        Ok(id)
    }

    fn read_saved(&self) -> Option<SavedSession> {
        let text = std::fs::read_to_string(self.data.join(SESSION_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn saved_id(&self, key: &SessionKey) -> Option<String> {
        let saved = self.read_saved()?;
        if saved.harness != self.launch.name {
            return None;
        }
        saved
            .sessions
            .into_iter()
            .find_map(|slot| (slot.key() == *key).then_some(slot.session_id))
    }

    /// Cancel this Instance's own turn, recording the withdrawal so it reads
    /// as given up (ADR-0016). `wire.cancel()` names no session (ADR-0008), so
    /// ask who holds it first. The caller is the frame loop and must not wait.
    fn cancel_own_turn(&self, instance: &str) {
        let ours = self
            .serving_instance
            .lock()
            .is_ok_and(|serving| serving.as_deref() == Some(instance));
        if !ours {
            return;
        }
        let Some(wire) = self.current_wire() else {
            return;
        };
        self.note_withdrawal(Some(instance.to_string()));
        wire.cancel();
    }

    /// Forget this Instance's ACP conversations so the next wake is
    /// `session/new`. Every lane opened with the old Instance Prompt
    /// (ADR-0012). In-memory ids and saved slots both go, or a restart loads it.
    pub fn drop_conversation(&self, instance: &str) {
        self.cancel_own_turn(instance);
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let keys: Vec<SessionKey> = state
            .sessions
            .keys()
            .filter(|key| key.instance == instance)
            .cloned()
            .collect();
        let dropped: Vec<String> = keys
            .iter()
            .filter_map(|key| state.sessions.remove(key).map(|opened| opened.id))
            .collect();
        self.forget_saved(instance);
        self.update_inspect(|inspect| {
            if inspect
                .session_id
                .as_deref()
                .is_some_and(|id| dropped.iter().any(|dropped| dropped == id))
            {
                inspect.session_id = None;
            }
        });
    }

    fn forget_saved(&self, instance: &str) {
        let Some(mut record) = self.read_saved() else {
            return;
        };
        record.sessions.retain(|slot| slot.instance != instance);
        if let Ok(text) = serde_json::to_string(&record) {
            let _ = std::fs::write(self.data.join(SESSION_FILE), format!("{text}\n"));
        }
    }

    fn drop_saved(&self, key: &SessionKey) {
        let Some(mut record) = self.read_saved() else {
            return;
        };
        record.sessions.retain(|slot| slot.key() != *key);
        if let Ok(text) = serde_json::to_string(&record) {
            let _ = std::fs::write(self.data.join(SESSION_FILE), format!("{text}\n"));
        }
    }

    fn save_session(&self, key: &SessionKey, id: &str) {
        let mut record = self.read_saved().unwrap_or(SavedSession {
            harness: self.launch.name.clone(),
            agent: None,
            sessions: Vec::new(),
            session_id: None,
        });
        record.harness = self.launch.name.clone();
        record.agent = self.inspect().agent;
        record.session_id = None;
        match record.sessions.iter_mut().find(|slot| slot.key() == *key) {
            Some(slot) => slot.session_id = id.to_string(),
            None => record.sessions.push(SavedSlot {
                instance: key.instance.clone(),
                character: key.character.clone(),
                blank: key.blank,
                session_id: id.to_string(),
            }),
        }
        if let Ok(text) = serde_json::to_string(&record) {
            let _ = std::fs::write(self.data.join(SESSION_FILE), format!("{text}\n"));
        }
    }
}

impl Completer for Session {
    fn complete(&self, request: &WakeRequest) -> Result<Reply, String> {
        if crate::model::tracing() {
            eprintln!("harness: prompt to {}", self.launch.name);
        }
        let reply = self.turn(request);
        if crate::model::tracing() {
            match &reply {
                Ok(reply) if reply.truncated => {
                    eprintln!("harness: reply cut off at the cap {}", reply.text)
                }
                Ok(reply) => eprintln!("harness: reply {}", reply.text),
                Err(why) => eprintln!("harness: {why}"),
            }
        }
        reply
    }
}

/// The one prompt the probe sends. Shaped like the last line of a Character
/// Prompt so a reply that does not parse is the Harness's doing, not the
/// prompt's. Asked for explicitly rather than left to the Harness.
const PROBE_PROMPT: &str =
    "Reply with exactly this one line and nothing else: Wave | Hello from the probe.";

/// How long shutdown waits for the child to be reaped before saying so.
const REAP: Duration = Duration::from_secs(2);

/// Attach the configured Harness and run one turn, with no overlay.
/// Exit 2 means never asked, 1 means asked and unanswered, 0 is `end_turn`.
/// A Harness that is not signed in names the command the user runs (ADR-0010).
pub fn run_probe() -> i32 {
    // No settings file on this path. `dev_flags::seed` is where the exported
    // timeout is read.
    crate::dev_flags::seed(&crate::settings::Settings::default());
    // The probe loads no settings file, so the exported variable is the only
    // source it has.
    let Some(target) = Target::from_settings(None, "") else {
        eprintln!("probe-harness: {VAR} is unset, so there is no Harness to attach");
        return 2;
    };
    let session = Arc::new(Session::new(
        target.launch,
        target.cwd,
        // The probe's own folder keeps the session file and Action Log out of a
        // real install. Memory cannot be isolated. The MCP server resolves it
        // from the data folder, so a probe `remember` writes the real `memory.md`.
        SessionDataDir::probe(),
        // Named, never answered. Only a click on the Chat surface may answer a
        // permission request (ADR-0017), and the probe has no surface. The ask
        // times out with the turn, which is itself the report.
        Arc::new(Box::new(|forwarded| match forwarded {
            Forwarded::Ask(ask) => println!(
                "  permission   {} [{}]",
                ask.title.as_deref().unwrap_or("—"),
                ask.request
            ),
            Forwarded::Form(form) => println!("  elicitation  {} [{}]", form.message, form.request),
            _ => {}
        }) as Forward),
    ));
    // Ctrl+C is ours before spawn so the child leaves this process group and
    // `kill_harness_tree` can reap `npx` grandchildren. The handler shuts the
    // session down. 130 is the shell's SIGINT code, not a probe verdict.
    let interrupted = session.clone();
    match ctrlc::set_handler(move || {
        interrupted.shutdown();
        std::process::exit(130);
    }) {
        Ok(()) => own_interrupt(),
        Err(why) => eprintln!(
            "probe-harness: could not catch interrupt: {why}; the Harness stays in this process group"
        ),
    }
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
    println!("  cwd          {}", attach_cwd_display(&session.cwd));
    println!("  data         {}", session.data.as_path().display());
    println!(
        "  timeout      turn {}s, attach {}s",
        session.timeout.as_secs(),
        session.attach_timeout().as_secs()
    );
    println!();

    println!("attach");
    let session_id = match session.attach(Some(&SessionKey {
        instance: "probe".to_string(),
        character: "probe".to_string(),
        blank: false,
    })) {
        Ok((_, id)) => id,
        // Nothing was asked, so this is configuration and not a turn. The
        // message names a missing binary, a login, or a refused `session/new`.
        // The code says only that the prompt never went out.
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
    // What the session was actually handed, not what was on offer. The label
    // never carries the loopback token.
    println!(
        "  mcp          {}",
        mcp_server(&handshake).map_or_else(|| "none".to_string(), |choice| choice.label())
    );
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
        // Reactive, because a probe is someone asking on purpose. Named for
        // the probe so the Action Log line cannot be read as a buddy's own wake.
        instance: "probe".to_string(),
        character: "probe".to_string(),
        reactive: true,
        // The probe sends its own fixed prompt, not a Character's, so the mode
        // it would have been assembled under decides nothing here.
        blank: false,
    }) {
        Ok(reply) => {
            let text = reply.text;
            println!(
                "  stop         {}",
                if reply.truncated {
                    "max_tokens"
                } else {
                    "end_turn"
                }
            );
            println!("  reply        {text}");
            // Reported, not part of the verdict. Whether a model obeys a
            // one-line format is the Director's problem. `end_turn` proved
            // the wire either way.
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

/// What the session stream said, into the Action Log, and a permission
/// request on to the Chat surface. Runs on the wire thread.
fn note_event(dir: &Path, forward: &Forward, event: Event) {
    match event {
        // A tool call and a usage tick are logged and never forwarded, so a
        // turn shows the surface no phases. ADR-0028 bounds what a later
        // surface may draw from events that already arrive here.
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
        Event::Plan(steps) => {
            // Guarded because `end_turn` clears the plan on every turn, and an
            // unguarded line would log a zero-step plan for turns that had none.
            if !steps.is_empty() {
                action_log::append(dir, "plan", json!({"entries": steps.len()}));
            }
            forward(Forwarded::Plan(steps));
        }
        Event::Usage { used, size } => {
            action_log::append(dir, "usage_update", json!({"used": used, "size": size}))
        }
        Event::Permission(ask) => {
            action_log::append(
                dir,
                "permission_request",
                json!({"request": ask.request, "title": ask.title, "kind": ask.kind}),
            );
            forward(Forwarded::Ask(ask));
        }
        Event::Elicitation(form) => {
            action_log::append(
                dir,
                "elicitation_create",
                json!({"request": form.request, "field": form.field}),
            );
            forward(Forwarded::Form(form));
        }
        Event::PermissionSettled { request, option } => {
            forward(Forwarded::Settled { request, option })
        }
        // Forwarded and not logged. The Action Log points at the Harness's own
        // session dump rather than copying it (CONTEXT.md). A thought chunk is
        // the part the Harness treats as disposable (ADR-0025).
        Event::Thought(line) => forward(Forwarded::Thought(line)),
    }
}

/// The Action Log line for what one reply parsed to.
/// The Shell writes it where it takes the wake out of `Slots`, because
/// `crates/core` parses and does no I/O.
pub fn note_parsed(instance: &str, wake: &Wake, reactive: bool, near_miss: Option<&str>) {
    let session = attached();
    let dir = session
        .as_ref()
        .map(|session| session.data.as_path().to_path_buf())
        .unwrap_or_else(ai_buddy_core::memory::data_dir);
    // Asked here rather than carried through `crates/core`. The caller has the
    // wake and not the words, and this already holds the session that knows
    // whose wake took it.
    let withdrawn_for = match (&session, wake) {
        (Some(session), Wake::Failed) => session.claim_withdrawn_wake(instance),
        _ => None,
    };
    let error = matches!(wake, Wake::Failed)
        .then(|| session.and_then(|session| session.inspect().last_error))
        .flatten();
    action_log::append(
        &dir,
        "parsed",
        parsed_fields(
            instance,
            wake,
            reactive,
            near_miss,
            error.as_deref(),
            withdrawn_for.as_deref(),
        ),
    );
}

/// The six answers: `near_miss`, `proposal`, `speech`, `failed`, `error`,
/// `withdrawn`. `near_miss` arrives as speech but is not `speech`.
/// `withdrawn` outranks `error` because our own cancel arrives as one.
fn parsed_fields(
    instance: &str,
    wake: &Wake,
    reactive: bool,
    near_miss: Option<&str>,
    error: Option<&str>,
    withdrawn_for: Option<&str>,
) -> Value {
    let (result, behavior) = match (near_miss, wake) {
        (Some(named), _) => ("near_miss", Some(named)),
        (None, Wake::Proposed(proposal)) if !proposal.behavior.is_empty() => {
            ("proposal", Some(proposal.behavior.as_str()))
        }
        (None, Wake::Proposed(_)) => ("speech", None),
        (None, Wake::Failed) if withdrawn_for.is_some() => ("withdrawn", None),
        (None, Wake::Failed) if error.is_some() => ("error", None),
        (None, Wake::Failed) => ("failed", None),
    };
    json!({
        "instance": instance,
        "wake": wake_kind(reactive),
        "result": result,
        "behavior": behavior,
        // The cancel we sent is not words the Harness chose, so a withdrawal
        // carries none. `withdrawn_for` is the whole account of that line.
        "error": withdrawn_for.is_none().then_some(error).flatten(),
        "withdrawn_for": withdrawn_for,
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

/// A Harness is the user's to install and never ours to ship (ADR-0018), so
/// the only fix is one the user makes outside the app, the same shape as
/// `not_authenticated` and for the same reason.
fn not_installed(command: &str) -> String {
    let install_hint = match command {
        "npx" => " Install Node.js from https://nodejs.org/.",
        "hermes" => " Install Hermes from https://hermes-agent.nousresearch.com/.",
        // The ACP registry still names https://block.github.io/goose/, which
        // now redirects to goose-docs.ai. The install page is the CLI instructions.
        "goose" => " Install Goose from https://goose-docs.ai/docs/getting-started/installation/.",
        "cursor-agent" => " Install Cursor from https://www.cursor.com/.",
        "grok" => " Install Grok from https://x.ai/.",
        "opencode" => " Install OpenCode from https://opencode.ai/.",
        _ => "",
    };
    format!("`{command}` is not installed; ai-buddy does not bundle a Harness.{install_hint}")
}

/// The command that logs the user in. The table outranks the handshake,
/// because ACP describes `authMethods` in prose and ADR-0018 hosts no
/// terminal to run a `terminal` method. A custom command keeps the adapter's text.
fn login_command(name: &str, handshake: &Handshake) -> String {
    named_login(name).map(str::to_string).unwrap_or_else(|| {
        handshake
            .auth_methods
            .first()
            .map(|method| {
                method
                    .description
                    .clone()
                    .unwrap_or_else(|| method.name.clone())
            })
            .unwrap_or_else(|| login_hint(name))
    })
}

/// The documented sign-in line for a named Harness, before any handshake.
/// Chat's Connect reads it at the pick, Settings after `-32000` (ADR-0022).
/// ai-buddy never runs it (ADR-0018).
pub(crate) fn login_hint(name: &str) -> String {
    named_login(name)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{name} (run it once in a terminal and sign in)"))
}

fn named_login(name: &str) -> Option<&'static str> {
    Some(match name {
        "claude" => "claude /login",
        "codex" => "codex login",
        // Not the line the handshake offers. Cursor describes "agent login",
        // and `agent` is what the binary calls itself, not the `cursor-agent`
        // the installer puts on `PATH`. Following it verbatim is a command not found.
        "cursor-agent" => "cursor-agent login",
        "grok" => "grok login",
        // No `goose login`. Provider setup is `goose configure`.
        "goose" => "goose configure",
        "hermes" => "hermes login",
        "opencode" => "opencode login",
        "pi" => "npx -y pi-acp@latest --terminal-login",
        _ => return None,
    })
}

/// The MCP server to hand this session. Loopback first (ADR-0023). No
/// `mcpCapabilities.http` on handshake means the stdio shim (ADR-0026).
/// Branch on that bit only. Token in the Authorization header, never URL or argv.
fn mcp_server(handshake: &Handshake) -> Option<McpChoice> {
    choose_mcp(handshake, crate::mcp_http::endpoint(), mcp_stdio())
}

/// The choice itself, with both candidates handed in. A test binary is not
/// named `ai-buddy` and has no sidecar beside it, so `mcp_stdio` finds nothing
/// there and the stdio branch would never be exercised.
fn choose_mcp(
    handshake: &Handshake,
    endpoint: Option<crate::mcp_http::Endpoint>,
    stdio: Option<McpLaunch>,
) -> Option<McpChoice> {
    if handshake.mcp_http {
        if let Some(endpoint) = &endpoint {
            return Some(McpChoice::Http {
                url: endpoint.url.clone(),
                authorization: endpoint.authorization(),
            });
        }
    }
    let mut launch = stdio?;
    launch.env = endpoint
        .map(|endpoint| endpoint.stdio_env())
        .unwrap_or_default();
    Some(McpChoice::Stdio(launch))
}

/// The stdio MCP server to hand the session, when one can be launched.
/// Read here rather than at construction, so a path typed in the window is
/// the one the next attach hands over. `AI_BUDDY_MCP_BIN` still outranks the file.
fn mcp_stdio() -> Option<McpLaunch> {
    let configured = crate::dev_flags::mcp_bin();
    mcp_launch(
        configured.as_deref(),
        std::env::current_exe().ok()?.as_path(),
    )
}

fn mcp_launch(configured: Option<&Path>, current_exe: &Path) -> Option<McpLaunch> {
    if let Some(path) = configured.filter(|path| path.is_file()) {
        return Some(McpLaunch {
            path: path.to_path_buf(),
            args: Vec::new(),
            env: Vec::new(),
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
            env: Vec::new(),
        });
    }
    // Sibling / configured path still win; this is how `cargo run` and a bundle
    // with no sidecar still hand the Harness a server. The loopback server
    // above is what a Harness that can take it gets instead.
    (current_exe.file_stem()? == "ai-buddy").then(|| McpLaunch {
        path: current_exe.to_path_buf(),
        args: vec!["--mcp-stdio".into()],
        env: Vec::new(),
    })
}

/// The one attachment, and what it needs to be opened again.
/// `forward` belongs to the Shell's window handle, not to any one child, and
/// `retarget` has no other way to get one.
struct Attachment {
    session: Option<Arc<Session>>,
    forward: Option<Arc<Forward>>,
}

static ATTACHED: Mutex<Attachment> = Mutex::new(Attachment {
    session: None,
    forward: None,
});

fn attachment() -> MutexGuard<'static, Attachment> {
    ATTACHED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Read the source, the variable, else `saved` from Settings, and hold the
/// Session until the row moves it or the process exits. Process-global because
/// the session is one per app (ADR-0008) and a Retarget rebuilds `DirectorSettings`.
pub(crate) fn attach(target: Option<Target>, forward: Forward) -> Option<Arc<Session>> {
    let mut slot = attachment();
    if slot.forward.is_some() {
        return slot.session.clone();
    }
    let forward = Arc::new(forward);
    slot.forward = Some(Arc::clone(&forward));
    slot.session = open(target, forward);
    slot.session.clone()
}

fn open(target: Option<Target>, forward: Arc<Forward>) -> Option<Arc<Session>> {
    target.map(|target| {
        Arc::new(Session::new(
            target.launch,
            target.cwd,
            SessionDataDir::app(),
            forward,
        ))
    })
}

/// What a Completer source row now in force asks of the attachment.
/// Alive is not an input. A set Harness is still the Completer (ADR-0008).
/// `Drop` is only a row that names no Harness, never a session failing.
#[derive(Debug, PartialEq, Eq)]
enum Reattach {
    /// The row still names what is attached, a dead one included. A fresh
    /// Session for the same command line would throw away the backoff the old
    /// one earned and respawn on every Apply.
    Stand,
    Drop,
    Open(Target),
}

fn reattach(attached: Option<&Target>, wanted: Option<Target>) -> Reattach {
    match wanted {
        None if attached.is_none() => Reattach::Stand,
        None => Reattach::Drop,
        Some(target) if attached == Some(&target) => Reattach::Stand,
        Some(target) => Reattach::Open(target),
    }
}

/// Re-open the attachment for the Completer source now in force.
/// A wire that dies is the Session's own business. `spawning` is the
/// Director's switch, so a session no wake will reach is not opened.
pub(crate) fn retarget(wanted: Option<Target>, spawning: bool) {
    let mut slot = attachment();
    // The probe and the tests never call `attach`, so there is no forward to
    // rebuild a Session with and nothing of theirs to move.
    let Some(forward) = slot.forward.clone() else {
        return;
    };
    let attached = slot.session.as_ref().map(|session| session.target());
    let opened = match reattach(attached.as_ref(), wanted) {
        Reattach::Stand => return,
        Reattach::Drop => None,
        Reattach::Open(target) => open(Some(target), forward),
    };
    // Swapped under the one lock `attached` reads. A gap here is a wake landing
    // on the HTTP Completer that nobody chose, which is what ADR-0008 refuses.
    let old = std::mem::replace(&mut slot.session, opened.clone());
    drop(slot);
    if let Some(old) = old {
        // On the calling thread, the UI thread for the only caller. Affordable
        // because `Msg::Shutdown` ends a turn in flight rather than being
        // swallowed by it, so `wait_for_exit` returns as fast as the kill does.
        old.shutdown();
    }
    match &opened {
        None => eprintln!("harness: detached; HTTP Completer is the Director's \"AI brain\""),
        Some(session) => {
            eprintln!("harness: {} is the Completer now", session.launch.line());
            if spawning {
                session.spawn_preflight();
            }
        }
    }
}

pub fn attached() -> Option<Arc<Session>> {
    attachment().session.clone()
}

/// Whether an attached Harness is actually answering, not merely configured.
/// `attach` holds the handle even when the child never spawned. Settings asks
/// this rather than `attached`, or a missing CLI freezes every Completer.
pub fn driving() -> bool {
    attached().is_some_and(|session| session.inspect().alive)
}

/// The error the attached Harness answered the last turn with, if it did.
/// The Completer seam hands core a bare `Err`, so a version refusal and an
/// unparsable reply both reach the Shell as `Wake::Failed`. Read only on failure.
pub fn last_error() -> Option<String> {
    attached().and_then(|session| session.inspect().last_error)
}

/// What `startup_lines` says about the attachment, if there is one.
/// `spawning` is whether one is actually coming. A line promising a spawn
/// the Director's switch has already refused is worse than no line.
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
        // Before the handshake, so this says what is on offer rather than
        // which one the session got. The probe's `mcp` line says the latter.
        match (crate::mcp_http::endpoint(), mcp_stdio()) {
            (Some(endpoint), Some(launch)) => format!(
                "harness: MCP server {}, or `{}` (relays here) for a Harness that advertises no mcpCapabilities.http",
                endpoint.url,
                launch.line()
            ),
            (Some(endpoint), None) => format!("harness: MCP server {}", endpoint.url),
            (None, Some(launch)) => {
                format!(
                    "harness: MCP server {} (no app endpoint to relay to)",
                    launch.line()
                )
            }
            (None, None) => "harness: no MCP server; the session gets no tools".to_string(),
        },
    ];
    // Startup cannot report a spawn that has not happened. `attach` runs on
    // the preflight thread and lands after these lines. Said here so the next
    // `harness:` line reads as this attachment's outcome.
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

    /// A tool-using Ask is not the Model API hop.
    #[test]
    fn an_unset_timeout_gives_a_harness_turn_minutes_not_the_http_hop() {
        crate::model::tests::with_env(None, None, None, || {
            crate::dev_flags::seed(&crate::settings::Settings::default());
            assert_eq!(turn_timeout(), Duration::from_secs(120));
            assert_ne!(
                turn_timeout(),
                crate::model::TIMEOUT,
                "the Model API hop is not a tool-using turn's budget"
            );
        });
    }

    /// The Model API field is not this budget.
    #[test]
    fn a_director_timeout_does_not_set_the_harness_turn() {
        crate::model::tests::with_env(None, None, None, || {
            crate::dev_flags::seed(&crate::settings::Settings {
                director_timeout_secs: "45".into(),
                ..Default::default()
            });
            assert_eq!(turn_timeout(), TURN_TIMEOUT);
        });
    }

    /// The Development field and `AI_BUDDY_HARNESS_TURN_TIMEOUT` still win.
    #[test]
    fn a_set_timeout_is_the_harness_turn_budget() {
        crate::model::tests::with_env(None, None, None, || {
            crate::dev_flags::seed(&crate::settings::Settings {
                harness_turn_timeout_secs: "45".into(),
                ..Default::default()
            });
            assert_eq!(turn_timeout(), Duration::from_secs(45));
        });
    }

    /// The fake ACP agent. This test binary re-executed with `script=<name>`
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

    fn record_open(count: Option<&Path>, message: &Value) {
        if let Some(cwd) = message.pointer("/params/cwd").and_then(Value::as_str) {
            record(count, &format!("cwd={cwd}"));
        }
        if message
            .pointer("/params/mcpServers")
            .and_then(Value::as_array)
            .is_some_and(|servers| !servers.is_empty())
        {
            record(count, "mcp");
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
                // A child that starts and then fails. The spawn is not
                // `Missing`.
                Some("initialize") if script == "die-initializing" => std::process::exit(3),
                Some("initialize") => {
                    if let Some(path) = count {
                        let _ = std::fs::write(
                            path.with_file_name("initialize.json"),
                            serde_json::to_vec(message.get("params").unwrap_or(&Value::Null))
                                .unwrap_or_default(),
                        );
                    }
                    say(json!({"jsonrpc": "2.0", "id": id, "result": {
                        "protocolVersion": 1,
                        "agentInfo": {"name": "fake-agent", "version": "0"},
                        "agentCapabilities": {"loadSession": script.starts_with("load"), "mcpCapabilities": {"http": true}},
                        "authMethods": [{"id": "fake", "name": "Fake login", "description": "fake --login"}],
                    }}));
                }
                Some("session/new") => {
                    record(count, "new");
                    record_open(count, &message);
                    if script == "die-opening" {
                        std::process::exit(3);
                    }
                    if script == "auth" && recorded(count, "new") == 1 {
                        say(
                            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "auth required"}}),
                        );
                    } else {
                        // A new session is a new id. Without the reset the fake
                        // would hand back whichever id the last prompt named.
                        // Counted so two Instances cannot share a minted id.
                        let n = recorded(count, "new");
                        session = if n <= 1 {
                            "fresh-id".to_string()
                        } else {
                            format!("fresh-id-{n}")
                        };
                        say(json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": session}}));
                    }
                }
                Some("session/load") => {
                    record(count, "load");
                    record_open(count, &message);
                    if message.pointer("/params/sessionId").and_then(Value::as_str) == Some("stale")
                    {
                        say(
                            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32602, "message": "no such session"}}),
                        );
                    } else {
                        say(json!({"jsonrpc": "2.0", "id": id, "result": {}}));
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
                        // The session the load claimed to restore is not
                        // there, so the first prompt refuses and the one
                        // after the reopen is served.
                        "load-dead" if prompts == 1 => stop(&id, "refusal"),
                        "load-refusal" => stop(&id, "refusal"),
                        "permission" => {
                            pending_prompt = Some(id);
                            say(
                                json!({"jsonrpc": "2.0", "id": 99, "method": "session/request_permission", "params": {
                                    "sessionId": &session,
                                    "toolCall": {
                                        "toolCallId": "t1",
                                        "title": "rm -rf /",
                                        "kind": "execute",
                                        "content": [{"type": "content", "content": {"type": "text", "text": "Delete everything?"}}],
                                        "rawInput": {"command": "rm -rf /"},
                                        "locations": [{"path": "/"}],
                                    },
                                    "options": [
                                        {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
                                        {"optionId": "reject", "name": "Reject", "kind": "reject_once"},
                                    ],
                                }}),
                            );
                        }
                        "elicitation" => {
                            pending_prompt = Some(id);
                            say(
                                json!({"jsonrpc": "2.0", "id": 100, "method": "elicitation/create", "params": {
                                    "sessionId": &session,
                                    "mode": "form",
                                    "message": "How should I approach this refactoring?",
                                    "requestedSchema": {
                                        "type": "object",
                                        "properties": {
                                            "strategy": {
                                                "type": "string",
                                                "enum": ["conservative", "balanced", "aggressive"]
                                            }
                                        },
                                        "required": ["strategy"]
                                    }
                                }}),
                            );
                        }
                        "slow" | "load-slow" if prompts == 1 => pending_prompt = Some(id),
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
                None if id == json!(100) => {
                    let action = message
                        .pointer("/result/action")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string();
                    record(count, &format!("elicit:{action}"));
                    if action == "accept" {
                        let value = message
                            .pointer("/result/content/strategy")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        record(count, &format!("elicit-value:{value}"));
                        chunk(&session, &format!("ok:{value}"));
                    } else if action == "decline" {
                        chunk(&session, "ok:declined");
                    }
                    if action == "accept" || action == "decline" {
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
        cwd: PathBuf,
        count: PathBuf,
        forwarded: Receiver<Forwarded>,
    }

    impl Fixture {
        fn new(script: &str) -> (Self, Session) {
            Self::build(script, false)
        }

        fn split(script: &str) -> (Self, Session) {
            Self::build(script, true)
        }

        fn build(script: &str, split: bool) -> (Self, Session) {
            let dir =
                std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let cwd = if split {
                let cwd = std::env::temp_dir()
                    .join(format!("ai-buddy-harness-cwd-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&cwd).unwrap();
                cwd
            } else {
                dir.clone()
            };
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
                Ok(AttachCwd(cwd.clone())),
                SessionDataDir::at(dir.clone()),
                Arc::new(Box::new(move |forwarded| {
                    let _ = tx.send(forwarded);
                }) as Forward),
            )
            .with_timeout(Duration::from_secs(10));
            (
                Self {
                    dir,
                    cwd,
                    count,
                    forwarded,
                },
                session,
            )
        }

        /// The next forwarded ask, or a panic naming what came instead.
        fn ask(&self) -> PermissionAsk {
            match self.forwarded.recv_timeout(Duration::from_secs(5)) {
                Ok(Forwarded::Ask(ask)) => ask,
                other => panic!("expected an ask, got {:?}", other.map(|_| "settled")),
            }
        }

        fn form(&self) -> ElicitationForm {
            match self.forwarded.recv_timeout(Duration::from_secs(5)) {
                Ok(Forwarded::Form(form)) => form,
                other => panic!("expected a form, got {:?}", other.map(|_| "other")),
            }
        }

        fn initialize_params(&self) -> Value {
            let path = self.dir.join("initialize.json");
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    return serde_json::from_str(&text).expect("initialize.json is JSON");
                }
                thread::sleep(Duration::from_millis(20));
            }
            panic!("initialize.json was never written");
        }

        /// The next forwarded settlement, the request and what won it.
        fn settled(&self) -> (String, Option<String>) {
            match self.forwarded.recv_timeout(Duration::from_secs(5)) {
                Ok(Forwarded::Settled { request, option }) => (request, option),
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

    /// A Session for a test that never reaches a permission request.
    fn silent() -> Arc<Forward> {
        Arc::new(Box::new(|_| {}) as Forward)
    }

    /// One reactive wake for `buddy-1` as BMO, which is every turn a test
    /// sends unless it is naming another identity.
    fn asking(prompt: &str) -> WakeRequest {
        asking_as("buddy-1", "bmo", prompt)
    }

    fn asking_as(instance: &str, character: &str, prompt: &str) -> WakeRequest {
        WakeRequest {
            prompt: prompt.to_string(),
            instance: instance.to_string(),
            character: character.to_string(),
            reactive: true,
            blank: false,
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
            if self.cwd != self.dir {
                let _ = std::fs::remove_dir_all(&self.cwd);
            }
        }
    }

    fn isolated_session(launch: Launch, dir: PathBuf, forward: Arc<Forward>) -> Session {
        Session::new(
            launch,
            Ok(AttachCwd(dir.clone())),
            SessionDataDir::at(dir),
            forward,
        )
    }

    fn launched(source: &str) -> Target {
        Target {
            launch: launch(Some(source)).unwrap(),
            cwd: Ok(AttachCwd::resolve("").expect("data_dir is always a path")),
        }
    }

    fn tmp_attach() -> AttachCwd {
        AttachCwd(PathBuf::from("/tmp"))
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
        assert_eq!(
            launch(Some("codex")).unwrap().argv,
            ["npx", "-y", "@agentclientprotocol/codex-acp@latest"]
        );
        assert_eq!(
            launch(Some("grok")).unwrap().argv,
            ["grok", "agent", "stdio"]
        );
        assert_eq!(launch(Some("hermes")).unwrap().argv, ["hermes", "acp"]);
        let goose = launch(Some("goose")).unwrap();
        assert_eq!(goose.name, "goose");
        assert_eq!(goose.argv, ["goose", "acp"]);
        assert_eq!(launch(Some("opencode")).unwrap().argv, ["opencode", "acp"]);
        assert_eq!(
            launch(Some("pi")).unwrap().argv,
            ["npx", "-y", "pi-acp@latest"]
        );
        let custom = launch(Some("  my-agent --acp  --quiet ")).unwrap();
        assert_eq!(custom.name, "my-agent");
        assert_eq!(custom.argv, ["my-agent", "--acp", "--quiet"]);
    }

    /// A thought is forwarded to the Chat surface and written nowhere. The
    /// Action Log points at the Harness's own session dump rather than copying
    /// it (CONTEXT.md). Streamed reasoning is the copy it refuses (ADR-0025).
    #[test]
    fn a_thought_reaches_the_surface_and_not_the_action_log() {
        let dir = std::env::temp_dir().join(format!("ai-buddy-thought-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, forwarded) = mpsc::channel();
        let forward = Box::new(move |what| {
            let _ = tx.send(what);
        }) as Forward;

        note_event(
            &dir,
            &forward,
            Event::Thought("Reading the roster".to_string()),
        );

        assert!(matches!(
            forwarded.try_recv(),
            Ok(Forwarded::Thought(line)) if line == "Reading the roster"
        ));
        assert!(!dir.join(action_log::FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The steps go to the reader on the Chat surface, and the Action Log
    /// keeps the count. The log points at the Harness's own session dump
    /// rather than copying it (CONTEXT.md). The step text is that copy.
    #[test]
    fn a_plan_reaches_the_surface_and_the_action_log_keeps_the_count() {
        let dir = std::env::temp_dir().join(format!("ai-buddy-plan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, forwarded) = mpsc::channel();
        let forward = Box::new(move |what| {
            let _ = tx.send(what);
        }) as Forward;

        note_event(
            &dir,
            &forward,
            Event::Plan(vec![PlanStep {
                content: "read the roster".to_string(),
                priority: "high".to_string(),
                status: "in_progress".to_string(),
            }]),
        );

        assert!(matches!(
            forwarded.try_recv(),
            Ok(Forwarded::Plan(steps)) if steps[0].content == "read the roster"
        ));
        let logged = std::fs::read_to_string(dir.join(action_log::FILE)).unwrap();
        assert!(logged.contains(r#""entries":1"#), "{logged}");
        assert!(!logged.contains("read the roster"), "{logged}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The clear `end_turn` fires on every turn. It has to reach the surface
    /// and leave the Action Log alone. A line per turn saying zero steps is
    /// noise about a turn that never planned.
    #[test]
    fn an_empty_plan_clears_the_surface_and_writes_no_log_line() {
        let dir = std::env::temp_dir().join(format!("ai-buddy-plan-end-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, forwarded) = mpsc::channel();
        let forward = Box::new(move |what| {
            let _ = tx.send(what);
        }) as Forward;

        note_event(&dir, &forward, Event::Plan(Vec::new()));

        assert!(matches!(
            forwarded.try_recv(),
            Ok(Forwarded::Plan(steps)) if steps.is_empty()
        ));
        assert!(!dir.join(action_log::FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Asserted through `reattach` rather than a live retarget, because the
    /// attachment is process-global. A set Harness is the Completer even if
    /// silent (ADR-0008). Only Off reaches `Drop`.
    #[test]
    fn only_the_source_row_moves_the_attachment_and_only_off_drops_it() {
        let hermes = launched("hermes");
        let opencode = launched("opencode");

        assert_eq!(reattach(None, None), Reattach::Stand);
        assert_eq!(
            reattach(Some(&hermes), Some(hermes.clone())),
            Reattach::Stand,
            "a dead child is the Session's own retry, not a reason to rebuild it"
        );
        // The preset and the command line it joins to are one Harness, so
        // re-picking the same one another way keeps the session it has.
        assert_eq!(
            reattach(Some(&hermes), Some(launched("hermes acp"))),
            Reattach::Stand
        );
        assert_eq!(
            reattach(Some(&hermes), Some(opencode.clone())),
            Reattach::Open(opencode.clone())
        );
        assert_eq!(
            reattach(None, Some(opencode.clone())),
            Reattach::Open(opencode)
        );
        assert_eq!(reattach(Some(&hermes), None), Reattach::Drop);
    }

    #[test]
    fn reattach_stands_on_the_same_resolved_cwd_and_opens_on_a_different_one() {
        crate::model::tests::with_env(None, None, None, || {
            let hermes = launched("hermes");
            let data = ai_buddy_core::memory::data_dir();
            let data_row = data.to_string_lossy().into_owned();
            let same_data = Target::from_settings(Some("hermes"), &data_row).unwrap();
            assert_eq!(
                reattach(Some(&hermes), Some(same_data)),
                Reattach::Stand,
                "empty and an explicit data_dir resolve equal"
            );

            let home = ai_buddy_core::memory::home_dir().expect("the test user has a home");
            let home_row = home.to_string_lossy().into_owned();
            let named_home = Target::from_settings(Some("hermes"), &home_row).unwrap();
            assert!(
                matches!(reattach(Some(&hermes), Some(named_home)), Reattach::Open(_)),
                "explicit home is a different project from empty"
            );

            let other_dir = std::env::temp_dir();
            let other = Target {
                launch: hermes.launch.clone(),
                cwd: AttachCwd::resolve(&other_dir.to_string_lossy()),
            };
            assert!(
                matches!(
                    reattach(Some(&hermes), Some(other.clone())),
                    Reattach::Open(_)
                ),
                "same Launch plus a different cwd must rebuild"
            );
            assert_eq!(reattach(Some(&other), Some(other.clone())), Reattach::Stand);
        });
    }

    #[test]
    fn attach_cwd_resolve_empty_is_data_dir_absolute_kept_relative_refused() {
        crate::model::tests::with_env(None, None, None, || {
            let data = ai_buddy_core::memory::data_dir();
            assert_eq!(AttachCwd::resolve("").unwrap().as_path(), data.as_path());
            assert_eq!(AttachCwd::resolve("   ").unwrap().as_path(), data.as_path());
            // `/tmp/...` is relative on Windows (`Path::is_absolute` wants a drive).
            let kept = std::env::temp_dir().join("kept");
            assert_eq!(
                AttachCwd::resolve(&kept.to_string_lossy())
                    .unwrap()
                    .as_path(),
                kept.as_path()
            );
            assert_eq!(
                AttachCwd::resolve("relative/project"),
                Err(CwdError::Relative(PathBuf::from("relative/project")))
            );

            let from_env = std::env::temp_dir().join("from-env");
            let from_file = std::env::temp_dir().join("from-file");
            std::env::set_var(CWD, from_env.as_os_str());
            let target =
                Target::from_settings(Some("hermes"), &from_file.to_string_lossy()).unwrap();
            assert_eq!(
                target.cwd.unwrap().as_path(),
                from_env.as_path(),
                "env outranks the file row"
            );
            std::env::remove_var(CWD);
        });
    }

    /// The Working directory row shows this path instead of naming the data
    /// folder in words (#913). A placeholder that stopped resolving would put
    /// a path on screen that the attach does not use.
    #[test]
    fn working_directory_placeholder_is_the_path_a_blank_row_runs_in() {
        crate::model::tests::with_env(None, None, None, || {
            assert_eq!(
                attach_cwd_placeholder(),
                ai_buddy_core::memory::data_dir().display().to_string()
            );

            let from_env = std::env::temp_dir().join("from-env");
            std::env::set_var(CWD, from_env.as_os_str());
            let owned = attach_cwd_placeholder();
            std::env::remove_var(CWD);
            assert_eq!(
                owned,
                from_env.display().to_string(),
                "a row the environment owns shows where it actually runs"
            );
        });
    }

    #[test]
    fn session_new_cwd_is_the_attach_dir_and_durable_files_live_in_the_store() {
        let (fx, session) = Fixture::split("happy");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(
            fx.count(&format!("cwd={}", fx.cwd.display())),
            1,
            "session/new cwd was not the attach dir"
        );
        assert_eq!(
            fx.count(&format!("cwd={}", fx.dir.display())),
            0,
            "session/new cwd was the store"
        );
        assert!(fx.dir.join(SESSION_FILE).is_file());
        assert!(fx.dir.join(action_log::FILE).is_file());
        assert!(!fx.cwd.join(SESSION_FILE).exists());
        assert!(!fx.cwd.join(action_log::FILE).exists());
        assert!(
            std::fs::read_dir(&fx.cwd).unwrap().next().is_none(),
            "ai-buddy writes nothing into the user's directory"
        );
        session.shutdown();
    }

    #[test]
    fn session_load_cwd_is_the_attach_dir_not_the_store() {
        let (fx, session) = Fixture::split("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"saved-ok"}]}"#,
        )
        .unwrap();
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(fx.count("load"), 1);
        assert_eq!(
            fx.count(&format!("cwd={}", fx.cwd.display())),
            1,
            "session/load cwd was not the attach dir"
        );
        assert_eq!(
            fx.count(&format!("cwd={}", fx.dir.display())),
            0,
            "session/load cwd was the store"
        );
        session.shutdown();
    }

    #[test]
    fn a_relative_cwd_fails_spawn_and_does_not_create_the_path() {
        let relative = PathBuf::from(format!("ai-buddy-rel-cwd-{}", uuid::Uuid::new_v4()));
        let data = std::env::temp_dir().join(format!("ai-buddy-rel-data-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&data).unwrap();
        let launch = Launch {
            name: "nope".into(),
            argv: vec!["/nonexistent/ai-buddy-no-such-harness".into()],
        };
        let session = Session::new(
            launch,
            Err(CwdError::Relative(relative.clone())),
            SessionDataDir::at(data.clone()),
            silent(),
        );
        let err = session.complete(&asking("hi")).unwrap_err();
        assert!(
            err.contains(&relative.display().to_string()),
            "spawn named the relative path, got {err}"
        );
        assert!(!relative.exists(), "spawn must not create a relative cwd");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn a_missing_cwd_fails_spawn_and_does_not_create_the_path() {
        let missing =
            std::env::temp_dir().join(format!("ai-buddy-missing-cwd-{}", uuid::Uuid::new_v4()));
        assert!(!missing.exists());
        let data =
            std::env::temp_dir().join(format!("ai-buddy-missing-data-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&data).unwrap();
        let launch = Launch {
            name: "nope".into(),
            argv: vec!["/nonexistent/ai-buddy-no-such-harness".into()],
        };
        let session = Session::new(
            launch,
            AttachCwd::resolve(&missing.to_string_lossy()),
            SessionDataDir::at(data.clone()),
            silent(),
        );
        let err = session.complete(&asking("hi")).unwrap_err();
        assert!(
            err.contains(&missing.display().to_string()),
            "spawn named the missing path, got {err}"
        );
        assert!(!missing.exists(), "spawn must not create a missing cwd");
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn spawn_creates_the_data_dir_we_own_before_checking_cwd() {
        let dir =
            std::env::temp_dir().join(format!("ai-buddy-default-cwd-{}", uuid::Uuid::new_v4()));
        assert!(!dir.exists());
        let launch = Launch {
            name: "nope".into(),
            argv: vec!["/nonexistent/ai-buddy-no-such-harness".into()],
        };
        let session = Session::new(
            launch,
            Ok(AttachCwd(dir.clone())),
            SessionDataDir::at(dir.clone()),
            silent(),
        );
        let _ = session.complete(&asking("hi"));
        assert!(
            dir.is_dir(),
            "empty default is a folder we own; spawn must create it"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn probe_layout_isolates_the_store_and_keeps_production_cwd() {
        crate::model::tests::with_env(None, None, None, || {
            let target = Target::from_settings(Some("hermes"), "").unwrap();
            let session =
                Session::new(target.launch, target.cwd, SessionDataDir::probe(), silent());
            assert!(
                session.data.as_path().ends_with("probe"),
                "got {}",
                session.data.as_path().display()
            );
            let data = ai_buddy_core::memory::data_dir();
            assert_eq!(
                session.cwd.as_ref().unwrap().as_path(),
                data.as_path(),
                "probe cwd must be production resolve, not the probe folder"
            );
            assert_ne!(session.data.as_path(), data.as_path());
        });
    }

    /// ADR-0010 rules 4 and 5, as code. The child gets our environment as
    /// it is, with no key set, no config dir moved, and no `--bare`.
    #[test]
    fn child_command_sets_no_env_and_passes_no_bare() {
        for name in [
            "claude",
            "codex",
            "cursor-agent",
            "goose",
            "grok",
            "hermes",
            "opencode",
            "pi",
        ] {
            let launch = launch(Some(name)).unwrap();
            let command = launch.command(&tmp_attach());
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

    /// Production change that would fail this. The child stays in the app's
    /// process group, so Ctrl+C SIGINTs Claude's adapter and it dumps
    /// `Query closed before response received` on the way down.
    #[cfg(unix)]
    #[test]
    fn the_harness_child_is_not_in_the_app_process_group() {
        let launch = Launch {
            name: "sleep".into(),
            argv: vec!["/bin/sleep".into(), "8".into()],
        };
        let mut command = launch.command(&tmp_attach());
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

    /// `kill_harness_tree` SIGKILLs a group without checking whose it is, so a
    /// child that never left ours takes this process with it. Asserted through
    /// the predicate. Calling it on our group would SIGKILL this test binary.
    #[cfg(unix)]
    #[test]
    fn our_own_group_is_never_killable() {
        let shared = sleep_pgid(false);
        let isolated = sleep_pgid(true);
        assert_eq!(
            Some(shared),
            pgid_of(std::process::id()),
            "the unisolated child under test has to share our group"
        );
        assert!(
            !crate::acp_wire::killable_group(shared as libc::pid_t),
            "SIGKILLing this group would take the probe, cargo test and the shell with it"
        );
        assert!(
            crate::acp_wire::killable_group(isolated as libc::pid_t),
            "an isolated child's group is the one kill_harness_tree exists to reap"
        );
    }

    /// The process group a `/bin/sleep` spawned under
    /// `apply_isolation(isolate)` landed in. The child is reaped before this
    /// returns; only its group is of interest.
    #[cfg(unix)]
    fn sleep_pgid(isolate: bool) -> i32 {
        let mut command = std::process::Command::new("/bin/sleep");
        command.arg("8");
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        apply_isolation(&mut command, isolate);
        let mut child = command.spawn().expect("sleep");
        let pgid = pgid_of(child.id()).expect("child pgid");
        let _ = child.kill();
        let _ = child.wait();
        pgid
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
        let mut command = launch.command(&tmp_attach());
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
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        let saved: SavedSession =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        assert_eq!(saved.sessions[0].session_id, "fresh-id");
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

    /// The file is a map of remembered ids, not one pointer the next buddy
    /// would inherit. Instance and Character together, because a retarget
    /// keeps the Instance and changes the Character Prompt.
    #[test]
    fn the_session_file_keys_the_id_by_instance_and_character() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        let slots = saved["sessions"]
            .as_array()
            .expect("remembered ids are keyed, not a single session_id");
        assert_eq!(slots.len(), 1, "{saved}");
        assert_eq!(slots[0]["instance"], json!("buddy-1"));
        assert_eq!(slots[0]["character"], json!("bmo"));
        assert_eq!(slots[0]["session_id"], json!("fresh-id"));
        session.shutdown();
    }

    /// Two Character Instances never share an ACP session, even when they
    /// are the same Character. One Harness child throughout.
    #[test]
    fn two_character_instances_do_not_share_an_acp_session() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "hi")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-2", "bmo", "hi")),
            Ok(Reply::whole("Hello"))
        );
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        assert_ne!(
            prompts[0]["session_id"], prompts[1]["session_id"],
            "Instance B continued Instance A's session: {prompts:?}"
        );
        assert_eq!(fx.count("spawn"), 1, "a second Completer was spawned");
        let saved: SavedSession =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        assert_eq!(saved.sessions.len(), 2, "{:?}", saved.sessions);
    }

    /// An Instance Prompt change cannot retrofit the opening turn (ADR-0012),
    /// so the next wake must be `session/new`, not a turn on the old id.
    #[test]
    fn dropping_a_conversation_opens_a_new_acp_session() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        session.drop_conversation("buddy-1");
        assert_eq!(
            session.complete(&asking("again")),
            Ok(Reply::whole("Hello"))
        );
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        assert_ne!(
            prompts[0]["session_id"], prompts[1]["session_id"],
            "the new Character Prompt continued the old ACP session: {prompts:?}"
        );
        assert_eq!(fx.count("new"), 2, "session/new was not asked again");
        let saved: SavedSession =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        assert_eq!(saved.sessions.len(), 1, "{:?}", saved.sessions);
        assert_eq!(saved.sessions[0].session_id, "fresh-id-2");
    }

    /// The Instance Prompt is in every lane, so dropping the Instance must
    /// forget Blank AI as well as the shaped conversation.
    #[test]
    fn dropping_an_instance_forgets_every_lane() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        let blank = WakeRequest {
            blank: true,
            ..asking("hi")
        };
        assert_eq!(session.complete(&blank), Ok(Reply::whole("Hello")));
        session.drop_conversation("buddy-1");
        assert_eq!(
            session.complete(&asking("again")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(session.complete(&blank), Ok(Reply::whole("Hello")));
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 4, "{prompts:?}");
        assert_ne!(
            prompts[0]["session_id"], prompts[2]["session_id"],
            "the shaped lane continued: {prompts:?}"
        );
        assert_ne!(
            prompts[1]["session_id"], prompts[3]["session_id"],
            "the blank lane continued: {prompts:?}"
        );
        assert_eq!(fx.count("new"), 4);
    }

    /// A restart must not `session/load` the id that save just tore down.
    #[test]
    fn dropping_a_conversation_forgets_the_saved_id() {
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"saved-ok"},{"instance":"buddy-2","character":"bmo","session_id":"id-b"}]}"#,
        )
        .unwrap();
        session.drop_conversation("buddy-1");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(fx.count("load"), 0, "the dropped id was loaded back");
        assert_eq!(fx.count("new"), 1);
        assert_eq!(
            session.complete(&asking_as("buddy-2", "bmo", "again")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(fx.count("load"), 1, "the other Instance was dropped too");
        session.shutdown();
    }

    /// Start a new session drops every live Instance, so every lane has to
    /// open with `session/new`. A restart between the two would load a
    /// torn-down id back, which is the box this is for.
    #[test]
    fn a_new_session_for_every_instance_loads_no_old_id() {
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"old-a"},{"instance":"buddy-2","character":"bmo","session_id":"old-b"}]}"#,
        )
        .unwrap();
        for instance in ["buddy-1", "buddy-2"] {
            session.drop_conversation(instance);
        }
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "hi")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-2", "bmo", "hi")),
            Ok(Reply::whole("Hello"))
        );
        session.shutdown();

        assert_eq!(fx.count("load"), 0, "a dropped id was loaded back");
        assert_eq!(fx.count("new"), 2, "each Instance opens its own session");
        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        for prompt in &prompts {
            assert_ne!(prompt["session_id"], json!("old-a"), "{prompts:?}");
            assert_ne!(prompt["session_id"], json!("old-b"), "{prompts:?}");
        }
        let saved: SavedSession =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        assert!(
            saved
                .sessions
                .iter()
                .all(|slot| slot.session_id != "old-a" && slot.session_id != "old-b"),
            "{:?}",
            saved.sessions
        );
    }

    /// Dropping one Instance's conversation must not mint a new session for
    /// another Instance that still holds its opening turn.
    #[test]
    fn dropping_one_instance_leaves_the_other_conversation() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "a")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-2", "bmo", "b")),
            Ok(Reply::whole("Hello"))
        );
        session.drop_conversation("buddy-1");
        assert_eq!(
            session.complete(&asking_as("buddy-2", "bmo", "c")),
            Ok(Reply::whole("Hello"))
        );
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 3, "{prompts:?}");
        assert_eq!(
            prompts[1]["session_id"], prompts[2]["session_id"],
            "Instance B was reopened when Instance A saved: {prompts:?}"
        );
    }

    /// Blank-AI mode is a different conversation. One session serving both
    /// would answer a blank prompt out of a Character it was told to forget,
    /// and the mode would measure a prompt it claims not to have sent.
    #[test]
    fn a_blank_wake_does_not_continue_the_shaped_session() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "hi")),
            Ok(Reply::whole("Hello"))
        );
        let blank = WakeRequest {
            blank: true,
            ..asking_as("buddy-1", "bmo", "hi")
        };
        assert_eq!(session.complete(&blank), Ok(Reply::whole("Hello")));
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        assert_ne!(
            prompts[0]["session_id"], prompts[1]["session_id"],
            "the blank wake continued the Character's session: {prompts:?}"
        );
        // Remembered apart too, so a restart resumes each mode where it was
        // rather than loading one into the other.
        let saved: SavedSession =
            serde_json::from_str(&std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap())
                .unwrap();
        assert_eq!(saved.sessions.len(), 2, "{:?}", saved.sessions);
        assert!(
            saved.sessions.iter().any(|slot| slot.blank),
            "the blank session is marked as one: {:?}",
            saved.sessions
        );
    }

    /// Switching away and back resumes the first Instance's session. The
    /// second Instance's id is still remembered.
    #[test]
    fn switching_back_resumes_the_first_instance_session() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "a")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-2", "bmo", "b")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "c")),
            Ok(Reply::whole("Hello"))
        );
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 3, "{prompts:?}");
        assert_eq!(prompts[0]["session_id"], prompts[2]["session_id"]);
        assert_ne!(prompts[0]["session_id"], prompts[1]["session_id"]);
        assert_eq!(
            fx.count("new"),
            2,
            "A was minted again: {}",
            fx.count("new")
        );
        assert_eq!(fx.count("spawn"), 1);
    }

    /// A Character switch is a different identity. Switching back loads the
    /// previous Character's session rather than leaving the new Character
    /// holding the old transcript.
    #[test]
    fn a_character_switch_does_not_keep_the_previous_transcript() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "a")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-1", "timber-wolf", "b")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "c")),
            Ok(Reply::whole("Hello"))
        );
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 3, "{prompts:?}");
        assert_ne!(
            prompts[0]["session_id"], prompts[1]["session_id"],
            "Timber Wolf continued BMO's session"
        );
        assert_eq!(prompts[0]["session_id"], prompts[2]["session_id"]);
        assert_eq!(fx.count("new"), 2);
    }

    /// A new Session reading the file restores each identity's id.
    #[test]
    fn a_restart_loads_each_instance_session_from_the_file() {
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"id-a"},{"instance":"buddy-2","character":"bmo","session_id":"id-b"}]}"#,
        )
        .unwrap();
        assert_eq!(
            session.complete(&asking_as("buddy-1", "bmo", "again")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            session.complete(&asking_as("buddy-2", "bmo", "again")),
            Ok(Reply::whole("Hello"))
        );
        session.shutdown();
        let prompts = fx.events("prompt");
        assert_eq!(prompts[0]["session_id"], json!("id-a"));
        assert_eq!(prompts[1]["session_id"], json!("id-b"));
        assert_eq!(fx.count("load"), 2);
        assert_eq!(fx.count("new"), 0, "restart minted instead of loading");
    }

    /// The old single pointer cannot be attributed, so it is not applied
    /// to every buddy.
    #[test]
    fn an_unattributed_legacy_id_is_not_applied_to_an_instance() {
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"session_id":"saved-ok","harness":"fake"}"#,
        )
        .unwrap();
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(fx.count("load"), 0, "the leftover id was applied");
        assert_eq!(fx.count("new"), 1);
        session.shutdown();
    }

    /// Off may race `spawn_preflight`. A spawn that lands after `shutdown`
    /// must not store a live child the HTTP Completer then cannot see.
    #[test]
    fn shutdown_refuses_a_later_turn() {
        let (_fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        session.shutdown();
        assert_eq!(
            session.complete(&asking("again")),
            Err("harness detached".to_string())
        );
    }

    /// `session_id` names the conversation, not the wake. Which Instance woke,
    /// and whether the user asked for it, come through the `WakeRequest`.
    #[test]
    fn the_prompt_event_names_the_instance_and_the_wake_kind() {
        let (fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        let proactive = WakeRequest {
            reactive: false,
            ..asking("nobody asked")
        };
        assert_eq!(session.complete(&proactive), Ok(Reply::whole("Hello")));
        session.shutdown();

        let prompts = fx.events("prompt");
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        assert_eq!(prompts[0]["instance"], json!("buddy-1"));
        assert_eq!(prompts[0]["wake"], json!("reactive"));
        assert_eq!(prompts[0]["chars"], json!(2));
        assert_eq!(prompts[1]["wake"], json!("proactive"));
        assert_eq!(prompts[1]["session_id"], json!("fresh-id"));
    }

    /// Without this the log said a prompt went out and never what came of it.
    /// Taking what the Harness proposed would have needed the trace flag.
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
            None,
            None,
        );
        assert_eq!(named["instance"], json!("buddy-1"));
        assert_eq!(named["wake"], json!("reactive"));
        assert_eq!(named["result"], json!("proposal"));
        assert_eq!(named["behavior"], json!("prowl"));

        // An empty name is the Engine's "talk and speak". The model chose to
        // talk rather than name a Behavior.
        let talked = parsed_fields("buddy-1", &spoke("hello?"), false, None, None, None);
        assert_eq!(talked["wake"], json!("proactive"));
        assert_eq!(talked["result"], json!("speech"));
        assert_eq!(talked["behavior"], json!(null));

        // The same shape as speech on the wire, and a different thing. The
        // name it named is what makes it readable as a miss.
        let missed = parsed_fields(
            "buddy-1",
            &spoke("prowll"),
            true,
            Some("prowll"),
            None,
            None,
        );
        assert_eq!(missed["result"], json!("near_miss"));
        assert_eq!(missed["behavior"], json!("prowll"));

        let failed = parsed_fields("buddy-1", &Wake::Failed, true, None, None, None);
        assert_eq!(failed["result"], json!("failed"));
        assert_eq!(failed["behavior"], json!(null));
        assert_eq!(failed["withdrawn_for"], json!(null));

        // The Harness answered, and what it answered was an error. That is
        // not the same outcome as a reply nothing could be parsed out of.
        let errored = parsed_fields(
            "buddy-1",
            &Wake::Failed,
            true,
            None,
            Some("harness: API Error: 400 does not support this model"),
            None,
        );
        assert_eq!(errored["result"], json!("error"));
        assert_eq!(
            errored["error"],
            json!("harness: API Error: 400 does not support this model")
        );

        // Our own cancel reaches this as an error, and the withdrawal is what
        // the line has to say.
        let withdrawn = parsed_fields(
            "buddy-1",
            &Wake::Failed,
            true,
            None,
            Some("harness stopped: cancelled"),
            Some("buddy-2"),
        );
        assert_eq!(withdrawn["result"], json!("withdrawn"));
        assert_eq!(withdrawn["withdrawn_for"], json!("buddy-2"));
        assert_eq!(withdrawn["error"], json!(null));
    }

    #[test]
    fn a_refusal_is_an_err() {
        let (fx, session) = Fixture::new("refusal");
        let reply = session.complete(&asking("hi"));
        assert!(
            reply.as_ref().is_err_and(|why| why.contains("refusal")),
            "{reply:?}"
        );
        // Only a loaded id is reopened. This session came from `session/new`,
        // so the refusal is the Harness's answer to the prompt.
        assert_eq!(fx.count("new"), 1);
        assert_eq!(fx.count("prompt"), 1);
        // The words the turn came back with outlive it, because every reader
        // downstream otherwise has only "no proposal" to say about a Harness
        // that is answering.
        assert!(
            session
                .inspect()
                .last_error
                .is_some_and(|why| why.contains("refusal")),
            "{:?}",
            session.inspect().last_error
        );
        session.shutdown();
    }

    #[test]
    fn garbage_between_messages_is_skipped() {
        let (_fx, session) = Fixture::new("garbage");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        session.shutdown();
    }

    #[test]
    fn initialize_payload_advertises_form_elicitation() {
        let (fx, session) = Fixture::new("hello");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        let params = fx.initialize_params();
        assert_eq!(
            params["clientCapabilities"]["elicitation"],
            json!({"form": {}})
        );
        session.shutdown();
    }

    #[test]
    fn elicitation_round_trip_records_the_chosen_option() {
        let (fx, session) = Fixture::new("elicitation");
        let session = Arc::new(session);
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        let form = fx.form();
        assert_eq!(form.message, "How should I approach this refactoring?");
        assert_eq!(form.field, "strategy");
        assert_eq!(form.options.len(), 3);
        session.answer_elicitation(&form.request, ElicitationAnswer::Accept("balanced".into()));
        assert_eq!(worker.join().unwrap(), Ok(Reply::whole("ok:balanced")));
        assert!(fx.wait_for("elicit:accept", 1));
        assert!(fx.wait_for("elicit-value:balanced", 1));
        assert_eq!(fx.settled(), (form.request, Some("balanced".to_string())));
        session.shutdown();
    }

    #[test]
    fn elicitation_round_trip_records_a_decline() {
        let (fx, session) = Fixture::new("elicitation");
        let session = Arc::new(session);
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        let form = fx.form();
        session.answer_elicitation(&form.request, ElicitationAnswer::Decline);
        assert_eq!(worker.join().unwrap(), Ok(Reply::whole("ok:declined")));
        assert!(fx.wait_for("elicit:decline", 1));
        assert_eq!(fx.settled(), (form.request, Some("decline".to_string())));
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
        assert_eq!(ask.title.as_deref(), Some("rm -rf /"));
        assert_eq!(ask.kind.as_deref(), Some("execute"));
        // What the tool call says about itself has to survive the trip, or
        // the surface is left asking the user to approve a kind.
        assert_eq!(ask.content, ["Delete everything?"]);
        assert_eq!(ask.input, Some(json!({"command": "rm -rf /"})));
        assert_eq!(ask.locations, ["/"]);
        assert_eq!(ask.options.len(), 2);
        session.answer_permission(&ask.request, "allow");
        assert_eq!(worker.join().unwrap(), Ok(Reply::whole("ok:allow")));
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
        // turn, with no option, because nothing was chosen.
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
        assert_eq!(
            session.complete(&asking("again")),
            Ok(Reply::whole("Hello"))
        );
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
        assert_eq!(
            session.complete(&asking("again")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(fx.count("spawn"), 2);
        assert!(session.inspect().alive);
        session.shutdown();
    }

    /// One count is shared by every way a wake goes unserved that leaves a
    /// child which might come back, so a Harness that dies is not respawned
    /// on the very next wake.
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

    /// The death that reaches nobody. `initialize` is answered and the child
    /// exits before `session/new`, so the loss surfaces in `open_session`.
    /// Charged all the same, or this Harness is respawned on every wake forever.
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

    /// The other half of one counter. A turn the child answered clears a death
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
        assert_eq!(
            session.complete(&asking("again")),
            Ok(Reply::whole("Hello"))
        );
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
        // Inside the gate. Fails fast, no second session/new on the wire.
        assert!(session.complete(&asking("hi")).is_err());
        assert_eq!(fx.count("new"), 1);
        session.shutdown();

        let (fx, session) = Fixture::new("auth");
        let session = session.with_auth_retry(Duration::ZERO);
        assert!(session.complete(&asking("hi")).is_err());
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(fx.count("new"), 2);
        assert_eq!(session.inspect().login, None);
        session.shutdown();
    }

    #[test]
    fn a_saved_session_is_loaded_and_a_stale_one_falls_back_to_new() {
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"saved-ok"}]}"#,
        )
        .unwrap();
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(fx.count("load"), 1);
        assert_eq!(fx.count("new"), 0);
        assert_eq!(session.inspect().session_id.as_deref(), Some("saved-ok"));
        session.shutdown();

        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"stale"}]}"#,
        )
        .unwrap();
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(fx.count("load"), 1);
        assert_eq!(fx.count("new"), 1);
        let saved = std::fs::read_to_string(fx.dir.join(SESSION_FILE)).unwrap();
        assert!(saved.contains("fresh-id"), "{saved}");
        session.shutdown();

        // Another Harness's session is not ours to load.
        let (fx, session) = Fixture::new("load");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"other","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"saved-ok"}]}"#,
        )
        .unwrap();
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        assert_eq!(fx.count("load"), 0);
        session.shutdown();
    }

    /// `hermes` answers a `session/load` it cannot honour with an empty
    /// success result, so the id is dead and only a turn says so. One reopen,
    /// and the retry lands on a session the Harness actually has.
    #[test]
    fn a_load_that_did_not_restore_reopens_once_and_the_retry_lands() {
        let (fx, session) = Fixture::new("load-dead");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"saved-ok"}]}"#,
        )
        .unwrap();
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
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

    /// The reopen is one attempt, not a ladder. A Harness that refuses the
    /// fresh session too has answered.
    #[test]
    fn a_reopened_session_that_refuses_again_is_a_refusal() {
        let (fx, session) = Fixture::new("load-refusal");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"saved-ok"}]}"#,
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

    /// `cancelled` is a stop reason, not evidence the load failed. Reopening
    /// would re-send a prompt carrying the Instance Prompt a save just
    /// replaced (ADR-0012). A cancel says nothing about the id.
    #[test]
    fn a_cancelled_turn_is_not_reopened_as_a_failed_load() {
        let (fx, session) = Fixture::new("load-slow");
        std::fs::write(
            fx.dir.join(SESSION_FILE),
            r#"{"harness":"fake","sessions":[{"instance":"buddy-1","character":"bmo","session_id":"saved-ok"}]}"#,
        )
        .unwrap();
        let session = Arc::new(session.with_timeout(Duration::from_secs(10)));
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        assert!(fx.wait_for("prompt", 1), "the first turn never went out");
        // The cancel arrives the way a save's does, through the path a newer
        // wake already takes. The loser writes its own line before the winner
        // gets the lock, so what it did with the cancel is settled here.
        assert_eq!(
            session.complete(&asking("again")),
            Ok(Reply::whole("Hello"))
        );
        let withdrawn = worker.join().unwrap().unwrap_err();
        assert!(withdrawn.contains("cancelled"), "{withdrawn}");
        assert_eq!(fx.count("load"), 1);
        assert_eq!(fx.count("new"), 0, "the cancelled turn threw the id away");
        assert_eq!(fx.count("prompt"), 2, "the cancelled turn was re-prompted");
        session.shutdown();
    }

    /// A binary that `PATH` has not got is not charged the respawn ladder. The
    /// next wake asks again. The ladder itself is untouched, which the three
    /// `backoff` reads below still hold.
    #[test]
    fn a_missing_binary_says_so_instead_of_backing_off() {
        const NOPE: &str = "/nonexistent/ai-buddy-no-such-harness";
        let dir = std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
        let launch = Launch {
            name: "nope".into(),
            argv: vec![NOPE.into()],
        };
        std::fs::create_dir_all(&dir).unwrap();
        let session = isolated_session(launch, dir.clone(), silent());
        assert_eq!(
            session.complete(&asking("hi")),
            Err(not_installed(NOPE)),
            "the wake was answered with an errno"
        );
        assert!(
            session.state.lock().unwrap().spawn_wait_until.is_none(),
            "a binary that is not there was charged a respawn wait"
        );
        // No wait means the next wake asks again rather than being refused, so
        // installing the CLI is picked up without a relaunch.
        assert_eq!(
            session.complete(&asking("hi")),
            Err(not_installed(NOPE)),
            "the second wake was refused by a backoff"
        );
        let inspect = session.inspect();
        assert_eq!(inspect.missing.as_deref(), Some(NOPE));
        assert!(!inspect.alive);
        assert_eq!(session.backoff(1), BACKOFF_FIRST);
        assert_eq!(session.backoff(2), Duration::from_secs(10));
        assert_eq!(session.backoff(40), BACKOFF_CAP);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Chat's first ReloadChat races preflight. The forwarded settle is how
    /// a missing launcher reaches an already-open surface (#726).
    #[test]
    fn spawn_preflight_forwards_when_the_launcher_is_missing() {
        const NOPE: &str = "/nonexistent/ai-buddy-no-such-harness";
        let dir = std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
        let (tx, rx) = mpsc::channel();
        let launch = Launch {
            name: "nope".into(),
            argv: vec![NOPE.into()],
        };
        let session = Arc::new(Session::new(
            launch,
            Ok(AttachCwd(dir.clone())),
            SessionDataDir::at(dir.clone()),
            Arc::new(Box::new(move |forwarded| {
                let _ = tx.send(forwarded);
            }) as Forward),
        ));
        session.spawn_preflight();
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Forwarded::AttachSettled) => {}
            other => panic!("expected AttachSettled, got {other:?}"),
        }
        let inspect = session.inspect();
        assert_eq!(inspect.missing.as_deref(), Some(NOPE));
        assert!(!inspect.alive);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Initializing gates chat during ACP handshake. Set true at spawn_preflight,
    /// false on success or failure.
    #[test]
    fn initializing_gates_chat_until_spawn_completes() {
        const NOPE: &str = "/nonexistent/ai-buddy-no-such-harness";
        let dir = std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
        let (tx, rx) = mpsc::channel();
        let launch = Launch {
            name: "nope".into(),
            argv: vec![NOPE.into()],
        };
        let session = Arc::new(Session::new(
            launch,
            Ok(AttachCwd(dir.clone())),
            SessionDataDir::at(dir.clone()),
            Arc::new(Box::new(move |forwarded| {
                let _ = tx.send(forwarded);
            }) as Forward),
        ));

        // Before spawn_preflight, initializing is false
        assert!(!session.inspect().initializing, "initializing starts false");

        session.spawn_preflight();

        // Immediately after spawn_preflight, initializing is true
        assert!(
            session.inspect().initializing,
            "initializing is true during spawn"
        );

        // Wait for attach to complete
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Forwarded::AttachSettled) => {}
            other => panic!("expected AttachSettled, got {other:?}"),
        }

        // After spawn fails, initializing is false
        let inspect = session.inspect();
        assert!(
            !inspect.initializing,
            "initializing is false after spawn fails"
        );
        assert!(!inspect.alive, "alive is false after spawn fails");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn initializing_clears_on_spawn_failed_not_just_missing() {
        let dir = std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
        let (tx, rx) = mpsc::channel();
        let launch = Launch {
            name: "fails".into(),
            argv: vec!["any".into()],
        };
        let session = Arc::new(Session::new(
            launch,
            Err(CwdError::Relative(PathBuf::from("relative/path"))),
            SessionDataDir::at(dir.clone()),
            Arc::new(Box::new(move |forwarded| {
                let _ = tx.send(forwarded);
            }) as Forward),
        ));

        assert!(!session.inspect().initializing);

        session.spawn_preflight();

        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Forwarded::AttachSettled) => {}
            other => panic!("expected AttachSettled, got {other:?}"),
        }

        assert!(!session.inspect().initializing);
        assert!(!session.inspect().alive);

        let _ = std::fs::remove_dir_all(dir);
    }

    /// `missing` has to be cleared by a spawn that fails, not only by one
    /// that works. Otherwise Settings tells the user to install what they
    /// just installed and never names the real failure.
    #[test]
    fn a_spawn_that_fails_for_another_reason_stops_saying_not_installed() {
        let (fx, session) = Fixture::new("die-initializing");
        session.note_missing();
        let reply = session.complete(&asking("hi"));
        assert!(
            reply.as_ref().is_err_and(|why| why.contains("initialize")),
            "{reply:?}"
        );
        let inspect = session.inspect();
        assert_eq!(inspect.missing, None, "the row still says not installed");
        assert!(!inspect.alive);
        // The child did start, so the ladder is still the right answer for it.
        assert!(session.state.lock().unwrap().spawn_wait_until.is_some());
        session.shutdown();
        let _ = std::fs::remove_dir_all(&fx.dir);
    }

    /// `codex` attaches through `npx` and logs in through `codex`, so `argv[0]`
    /// being present says nothing about the vendor CLI. What is named is the
    /// file that was looked for, never the preset.
    #[test]
    fn a_missing_launcher_is_not_a_missing_vendor_cli() {
        let launch = launch(Some("codex")).unwrap();
        assert_eq!(launch.argv[0], "npx", "the codex preset stopped using npx");
        let dir = std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
        let session = isolated_session(launch, dir, silent());
        assert_eq!(session.note_missing(), not_installed("npx"));
        assert_eq!(session.inspect().missing.as_deref(), Some("npx"));
    }

    /// Documents the missing-binary contract: `missing` = `argv[0]` for every
    /// preset. npx adapters (claude, codex, pi) report `npx` missing, not the
    /// vendor CLI. First-party CLIs (cursor-agent, goose, grok, hermes,
    /// opencode) report their own name.
    #[test]
    fn each_preset_reports_its_argv_0_as_missing() {
        let cases = [
            ("claude", "npx"),
            ("codex", "npx"),
            ("pi", "npx"),
            ("cursor-agent", "cursor-agent"),
            ("goose", "goose"),
            ("grok", "grok"),
            ("hermes", "hermes"),
            ("opencode", "opencode"),
        ];
        for (preset, expected_argv0) in cases {
            let launch = launch(Some(preset)).unwrap();
            assert_eq!(
                launch.argv[0], expected_argv0,
                "preset {preset} should have argv[0]={expected_argv0}"
            );
        }
    }

    #[test]
    fn missing_binary_messages_include_install_urls() {
        let cases = [
            ("npx", "nodejs.org"),
            ("hermes", "hermes-agent.nousresearch.com"),
            ("goose", "goose-docs.ai/docs/getting-started/installation"),
            ("cursor-agent", "cursor.com"),
            ("grok", "x.ai"),
            ("opencode", "opencode.ai"),
        ];
        for (command, url_part) in cases {
            let message = not_installed(command);
            assert!(
                message.contains(url_part),
                "missing {command} should mention {url_part}, got: {message}"
            );
        }
    }

    /// ADR-0016's newest-wins, at the Harness seam. The Poke that arrives
    /// under a turn takes it rather than being refused.
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
        assert_eq!(
            session.complete(&asking("again")),
            Ok(Reply::whole("Hello"))
        );
        assert_eq!(
            fx.count("cancel"),
            1,
            "the displaced turn was not cancelled"
        );
        assert_eq!(fx.count("prompt"), 2, "the second prompt never went out");
        // A withdrawal, not a failure. Cancelled by name in the log, no
        // `refused` line, and no respawn backoff charged.
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

    /// Switching the AI source commits on the UI thread, so `retarget` must
    /// not wait on the child it is dropping. That is cheap only because
    /// `Msg::Shutdown` ends the turn in flight instead of being swallowed by it.
    #[test]
    fn retarget_does_not_wait_out_a_turn_in_flight() {
        let (fx, session) = Fixture::new("slow");
        let session = Arc::new(session);
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        // The prompt is on the wire and unanswered. `shutdown` posts into a
        // `turn`, not into `serve`.
        assert!(fx.wait_for("prompt", 1), "the turn never went out");
        {
            let mut slot = attachment();
            slot.forward = Some(silent());
            slot.session = Some(Arc::clone(&session));
        }
        let start = Instant::now();
        // Off is `None`, which `reattach` reads as `Drop`. The session goes and
        // nothing replaces it.
        retarget(None, false);
        let waited = start.elapsed();
        // The session is process-global; leave the slot as the other tests
        // expect to find it.
        {
            let mut slot = attachment();
            slot.session = None;
            slot.forward = None;
        }
        // Fast and reaped are the same fact. `shutdown` returns when the
        // wire thread ends after `kill_harness_tree`, or when `REAP` runs
        // out at 2s. Under half a second is killed. 2s is still running.
        assert!(
            waited < Duration::from_millis(500),
            "retarget waited {waited:?} on the dropped session"
        );
        assert!(attached().is_none(), "the Off pick did not take");
        // And the child really was killed rather than left behind. The wire
        // thread ends only after `kill_harness_tree`, and the turn's caller
        // only returns once that thread has hung up on it.
        assert!(
            worker.join().unwrap().is_err(),
            "the turn outlived the kill"
        );
        assert!(
            !session.inspect().alive,
            "the dropped session still reads live"
        );
    }

    /// One child serves every Instance (ADR-0008), so buddy B's wake takes
    /// buddy A's turn without A's own slot ever having been superseded.
    /// Nothing raised A's abandon flag, so A took a wake that read as broken.
    #[test]
    fn a_turn_taken_for_another_instance_is_recorded_as_withdrawn() {
        let (fx, session) = Fixture::new("slow");
        let session = Arc::new(session.with_timeout(Duration::from_secs(10)));
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        assert!(fx.wait_for("prompt", 1), "the first turn never went out");
        let poke = WakeRequest {
            instance: "buddy-2".to_string(),
            ..asking("again")
        };
        assert_eq!(session.complete(&poke), Ok(Reply::whole("Hello")));
        assert!(worker.join().unwrap().is_err(), "the turn was not taken");

        // The loser's own line, written while it still held the lock, so it
        // comes before the winner's.
        let turns = fx.events("turn");
        assert_eq!(turns[0]["stop"], json!("cancelled"), "{turns:?}");
        assert_eq!(turns[0]["withdrawn_for"], json!("buddy-2"), "{turns:?}");

        // And what `note_parsed` writes for the wake that lost the session.
        // Called here rather than through the Shell's entry point, which reads
        // the process-global attached session.
        let withdrawn = session.claim_withdrawn_wake("buddy-1");
        let parsed = parsed_fields(
            "buddy-1",
            &Wake::Failed,
            true,
            None,
            None,
            withdrawn.as_deref(),
        );
        assert_eq!(parsed["result"], json!("withdrawn"), "{parsed}");
        assert_eq!(parsed["withdrawn_for"], json!("buddy-2"), "{parsed}");
        // One wake, one withdrawal. The next wake this Instance takes is its
        // own however that one ends.
        assert_eq!(session.claim_withdrawn_wake("buddy-1"), None);
        session.shutdown();
    }

    /// Saving an Instance Prompt cancels the reply in flight as well as
    /// tearing the conversation down (ADR-0012). Dropping the conversation
    /// alone leaves the old ACP session generating on a replaced transcript.
    #[test]
    fn saving_an_instance_prompt_cancels_that_instances_turn() {
        let (fx, session) = Fixture::new("slow");
        let session = Arc::new(session.with_timeout(Duration::from_secs(10)));
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        assert!(fx.wait_for("prompt", 1), "the turn never went out");
        session.drop_conversation("buddy-1");
        assert!(
            fx.wait_for("cancel", 1),
            "the saved-over turn was not cancelled"
        );
        let stopped = worker.join().unwrap().unwrap_err();
        assert!(stopped.contains("cancelled"), "{stopped}");
        // A withdrawal, not a fault. The turn line says who the turn was given
        // up for, and the Chat surface is told nothing broke.
        let turns = fx.events("turn");
        assert_eq!(turns[0]["stop"], json!("cancelled"), "{turns:?}");
        assert_eq!(turns[0]["withdrawn_for"], json!("buddy-1"), "{turns:?}");
        assert_eq!(session.inspect().last_error, None);
        assert_eq!(
            session.claim_withdrawn_wake("buddy-1").as_deref(),
            Some("buddy-1")
        );
        session.shutdown();
    }

    /// One child serves every Instance (ADR-0008), so a save that cancelled
    /// whatever held the turn would stop buddy B mid-reply because buddy A
    /// edited a prompt B has nothing to do with.
    #[test]
    fn saving_one_instances_prompt_leaves_another_instances_turn_alone() {
        let (fx, session) = Fixture::new("slow");
        let session = Arc::new(session.with_timeout(Duration::from_secs(10)));
        let worker = {
            let session = Arc::clone(&session);
            thread::spawn(move || session.complete(&asking("hi")))
        };
        assert!(fx.wait_for("prompt", 1), "the turn never went out");
        session.drop_conversation("buddy-2");
        // Long enough for a cancel to have reached the child and been
        // recorded. The one below is recorded well inside this wait.
        thread::sleep(Duration::from_millis(250));
        assert_eq!(
            fx.count("cancel"),
            0,
            "another Instance's turn was cancelled"
        );
        assert!(!worker.is_finished(), "another Instance's turn was ended");
        // And the same save for the Instance that does hold the turn ends it,
        // so the assertions above are about who saved rather than about a
        // cancel that never works.
        session.drop_conversation("buddy-1");
        assert!(
            fx.wait_for("cancel", 1),
            "the owning Instance's turn survived"
        );
        assert!(
            worker.join().unwrap().is_err(),
            "the turn was not cancelled"
        );
        session.shutdown();
    }

    /// The withdrawal a later supersede must not erase. The `parsed` line is
    /// written on the Shell side, frames after the turn ended, so a third
    /// wake may already have taken the session from a fourth.
    #[test]
    fn a_later_supersede_does_not_erase_an_unclaimed_withdrawal() {
        let dir = std::env::temp_dir().join(format!("ai-buddy-harness-{}", uuid::Uuid::new_v4()));
        let launch = Launch {
            name: "nope".into(),
            argv: vec!["/nonexistent/ai-buddy-no-such-harness".into()],
        };
        std::fs::create_dir_all(&dir).unwrap();
        let session = isolated_session(launch, dir.clone(), silent());

        session.note_withdrawal(Some("buddy-2".to_string()));
        assert_eq!(
            session
                .claim_withdrawn_turn("buddy-1", CANCELLED)
                .as_deref(),
            Some("buddy-2")
        );

        // buddy-3 takes it from someone else, and the handover it does not win
        // clears the pending slot. Neither is buddy-1's.
        session.note_withdrawal(Some("buddy-3".to_string()));
        session.note_withdrawal(None);

        let withdrawn = session.claim_withdrawn_wake("buddy-1");
        assert_eq!(withdrawn.as_deref(), Some("buddy-2"));
        let parsed = parsed_fields(
            "buddy-1",
            &Wake::Failed,
            true,
            None,
            None,
            withdrawn.as_deref(),
        );
        assert_eq!(parsed["result"], json!("withdrawn"), "{parsed}");
        assert_eq!(
            session.claim_withdrawn_wake("buddy-1"),
            None,
            "claimed twice"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The one wake that waits instead. An ambient tick cancelling the Poke
    /// it arrived behind would be worse than the refusal.
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
        // Stop the fake child before joining. The assertion above proves the
        // refusal while the reactive turn is live; waiting for its timeout
        // would add the full three-second budget to this unit test.
        session.shutdown();
        let _ = worker.join();

        // The refused wake sent no prompt, so without a line of its own the
        // `parsed` line the Shell writes for it would read against the prompt
        // the wake before it logged.
        let refused = fx.events("refused");
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0]["instance"], json!("buddy-1"));
        assert_eq!(refused[0]["wake"], json!("proactive"));
        assert_eq!(refused[0]["why"], json!("harness busy"));
        assert_eq!(fx.events("prompt").len(), 1, "the refused wake sent none");
    }

    /// The probe's two exit codes are its two phases. 1 is a turn that did
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
        std::fs::create_dir_all(&dir).unwrap();
        let launch = Launch {
            name: "nope".into(),
            argv: vec!["/nonexistent/ai-buddy-no-such-harness".into()],
        };
        assert_eq!(probe(&isolated_session(launch, dir.clone(), silent())), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// `shutdown` kills and waits; a zero-length wait can only succeed if
    /// that wait already finished.
    #[test]
    fn the_probe_waits_for_the_child_to_be_reaped() {
        let (_fx, session) = Fixture::new("happy");
        assert_eq!(session.complete(&asking("hi")), Ok(Reply::whole("Hello")));
        let wire = session.current_wire().expect("attached");
        session.shutdown();
        assert!(
            wire.wait_for_exit(Duration::ZERO),
            "the wire thread outlived the reap"
        );
    }

    #[test]
    fn login_command_takes_the_table_for_a_named_harness_and_the_handshake_for_a_custom_one() {
        let hint = |description: Option<&str>| Handshake {
            auth_methods: vec![crate::acp_wire::AuthHint {
                name: "ChatGPT".into(),
                description: description.map(str::to_string),
            }],
            ..Default::default()
        };
        assert_eq!(
            login_command("codex", &hint(Some("Sign in with ChatGPT"))),
            "codex login"
        );
        assert_eq!(
            login_command("x", &hint(Some("run x login"))),
            "run x login"
        );
        assert_eq!(login_command("x", &hint(None)), "ChatGPT");
        assert_eq!(
            login_command("x", &Handshake::default()),
            "x (run it once in a terminal and sign in)"
        );
    }

    /// Chat's Connect button answers with this line instead of spawning the
    /// login itself, and it answers at the moment of the pick, before any
    /// handshake. A named row without one would leave that answer a shrug.
    #[test]
    fn every_named_harness_documents_a_login_for_the_users_own_terminal() {
        for name in [
            "claude",
            "codex",
            "cursor-agent",
            "goose",
            "grok",
            "hermes",
            "opencode",
            "pi",
        ] {
            assert!(launch(Some(name)).is_some(), "{name} is not a named row");
            let hint = login_hint(name);
            assert!(
                !hint.contains("run it once"),
                "{name} falls through to the unnamed hint: {hint}"
            );
            assert_eq!(hint, login_command(name, &Handshake::default()));
        }
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

    /// ADR-0023's branch. The loopback server for a Harness whose ACP
    /// `initialize` advertised `mcpCapabilities.http`, and never for one that
    /// did not. A future Hermes with that bit set would get the HTTP server directly.
    #[test]
    fn the_loopback_server_goes_only_to_a_harness_that_advertised_http_mcp() {
        let (calls, _held) = mpsc::channel();
        let endpoint = crate::mcp_http::serve(calls).expect("loopback binds");

        let http = Handshake {
            mcp_http: true,
            ..Handshake::default()
        };
        match mcp_server(&http) {
            Some(McpChoice::Http { url, authorization }) => {
                assert_eq!(url, endpoint.url);
                assert!(authorization.starts_with("Bearer "));
            }
            other => panic!(
                "expected the loopback server, got {:?}",
                other.map(|c| c.label())
            ),
        }

        let stdio_only = Handshake::default();
        assert!(
            !matches!(mcp_server(&stdio_only), Some(McpChoice::Http { .. })),
            "a Harness whose initialize advertises no mcpCapabilities.http is never handed a URL"
        );
    }

    /// The stdio server is a shim that dials the app, so the choice has to
    /// carry the endpoint the shim reads, in the environment, never in the
    /// line the Action Log takes.
    #[test]
    fn the_stdio_server_is_handed_the_endpoint_in_its_environment() {
        let (calls, _held) = mpsc::channel();
        let endpoint = crate::mcp_http::serve(calls).expect("loopback binds");

        let sidecar = McpLaunch {
            path: PathBuf::from("/opt/ai-buddy-mcp"),
            args: Vec::new(),
            env: Vec::new(),
        };
        let Some(McpChoice::Stdio(launch)) = choose_mcp(
            &Handshake::default(),
            Some(endpoint.clone()),
            Some(sidecar.clone()),
        ) else {
            panic!("no stdio server to hand over");
        };
        let value = |name: &str| {
            launch
                .env
                .iter()
                .find(|(var, _)| var == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("{name} is not in the environment"))
        };
        assert_eq!(value(ai_buddy_mcp_server::URL_VAR), endpoint.url);
        assert_eq!(
            format!("Bearer {}", value(ai_buddy_mcp_server::TOKEN_VAR)),
            endpoint.authorization()
        );
        assert!(
            !launch
                .line()
                .contains(&value(ai_buddy_mcp_server::TOKEN_VAR)),
            "the token reached the line the Action Log takes"
        );

        // No app serving loopback is a shim that fails visibly, not one
        // dialling an endpoint it invented.
        let Some(McpChoice::Stdio(unreachable)) =
            choose_mcp(&Handshake::default(), None, Some(sidecar))
        else {
            panic!("no stdio server to hand over");
        };
        assert!(unreachable.env.is_empty());
    }
}
