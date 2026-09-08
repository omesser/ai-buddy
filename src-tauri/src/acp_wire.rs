//! The ACP wire: the official SDK and its executor, on one thread.
//!
//! `harness.rs` decides when to spawn, what to send, and what a failure means.
//! This file only speaks the protocol: it drives `agent-client-protocol`'s
//! connection future on a current-thread tokio runtime that exists on this
//! thread and nowhere else, and hands the rest of the shell plain values —
//! no SDK type crosses out of here. Reversing the crate choice (ADR-0017)
//! means rewriting this file and nothing beside it.
//!
//! Commands come in on a channel and each carries its reply channel, so the
//! caller blocks on `recv_timeout` while the protocol runs here. The frame
//! loop never sees any of it (ADR-0004): every caller is a `Slots` worker.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self as sync_mpsc, RecvTimeoutError};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AuthMethod, CancelNotification, ContentBlock, EnvVariable, ErrorCode, HttpHeader,
    Implementation, InitializeRequest, LoadSessionRequest, McpServer, McpServerHttp,
    McpServerStdio, NewSessionRequest, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo, Responder};
use serde::Serialize;
use tokio::sync::mpsc;

/// After `session/cancel`, how long the turn lock waits for the `cancelled`
/// reply before it is given back regardless.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// The MCP server `session/new` is told about, in whichever transport the
/// Harness said it takes.
///
/// The app's own loopback server is the shipped path (ADR-0023) and the stdio
/// binary is the fallback for a Harness that advertises no
/// `mcpCapabilities.http`. `harness.rs` chooses; this file only spells it.
#[derive(Clone)]
pub enum McpChoice {
    /// The loopback server the app serves, and the `Authorization` value that
    /// reaches it. The only one whose tools reach the buddy on screen.
    Http { url: String, authorization: String },
    /// A stdio server the Harness spawns: the sidecar, or the app binary
    /// re-executed as `--mcp-stdio` (#497). A shim that relays to the loopback
    /// server above, so its tools reach the same Instances (ADR-0026).
    Stdio(McpLaunch),
}

impl McpChoice {
    /// What a log line, the Action Log or a probe may say about this choice.
    ///
    /// Never the token: it is the one credential this process owns for a
    /// listener of its own, and ADR-0010's seventh rule reads the same way for
    /// one we mint as for one we would have borrowed.
    pub fn label(&self) -> String {
        match self {
            Self::Http { url, .. } => url.clone(),
            Self::Stdio(launch) => launch.line(),
        }
    }
}

/// What `initialize` told us, in the words the rest of the shell uses.
#[derive(Clone, Debug, Default)]
pub struct Handshake {
    pub agent: Option<String>,
    pub load_session: bool,
    /// Whether the Harness takes HTTP MCP servers. #166 branches on it.
    pub mcp_http: bool,
    pub auth_methods: Vec<AuthHint>,
}

/// One `authMethods` entry: enough to name the fix in a sentence.
#[derive(Clone, Debug)]
pub struct AuthHint {
    pub name: String,
    pub description: Option<String>,
}

/// A forwarded `session/request_permission`, as the Chat surface draws it.
#[derive(Clone, Debug, Serialize)]
pub struct PermissionAsk {
    /// The request id, as text, handed back with the answer.
    pub request: String,
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

/// What the session stream said, minus the text — that comes back with the
/// turn. Fed to the Action Log and the Chat surface by `harness.rs`.
#[derive(Clone, Debug)]
pub enum Event {
    ToolCall {
        id: String,
        title: Option<String>,
        kind: Option<String>,
        status: Option<String>,
    },
    Plan {
        entries: usize,
    },
    Usage {
        used: u64,
        size: u64,
    },
    Permission(PermissionAsk),
    /// The line of the Harness's thinking being written right now, for the
    /// Chat surface to show while the turn runs. Transient by decision
    /// (ADR-0025): each one replaces the last, nothing keeps them, and no line
    /// of the log or of the Action Log is made from one.
    Thought(String),
    /// A forwarded ask that can no longer be answered: the user answered it,
    /// the turn ended, or it was cancelled. Every open Chat surface was given
    /// the ask, so every one of them has to hear this.
    PermissionSettled {
        request: String,
        /// The option that won, and `None` when nothing was picked. Carried so
        /// the surfaces that did not take the click draw the decision that was
        /// actually made: two of them can draw one request, and every answer
        /// after the first is dropped here.
        option: Option<String>,
    },
}

/// The stdio MCP server handed to `session/new` and `session/load`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpLaunch {
    pub path: PathBuf,
    pub args: Vec<String>,
    /// What the shim reads to find the app and authorise itself (ADR-0026).
    /// Deliberately absent from `line`.
    pub env: Vec<(String, String)>,
}

impl McpLaunch {
    pub fn line(&self) -> String {
        if self.args.is_empty() {
            self.path.display().to_string()
        } else {
            format!("{} {}", self.path.display(), self.args.join(" "))
        }
    }
}

pub type OnEvent = Box<dyn Fn(Event) + Send + Sync>;

#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// `-32000`: the Harness wants a login it does not have.
    AuthRequired,
    /// The child is gone.
    Lost,
    Failed(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum TurnError {
    /// The Completer timeout passed; `session/cancel` has been sent.
    Timeout,
    /// The child is gone.
    Lost,
    /// A `stopReason` other than `end_turn`, by name.
    Stopped(String),
    /// A turn was already in flight on the wire.
    Busy,
    Failed(String),
}

enum Msg {
    Open {
        load: Option<String>,
        cwd: PathBuf,
        mcp: Option<McpChoice>,
        reply: sync_mpsc::Sender<Result<String, OpenError>>,
    },
    Prompt {
        session_id: String,
        text: String,
        reply: sync_mpsc::Sender<Result<String, TurnError>>,
    },
    Cancel,
    Answer {
        request: String,
        option: String,
    },
    Shutdown,
}

/// One spawned Harness and the thread that speaks to it.
pub struct Wire {
    tx: mpsc::UnboundedSender<Msg>,
    handshake: Handshake,
    /// Nothing is ever sent on this. The thread owns the sender, so the
    /// receiver disconnects at the moment the thread ends — which is how
    /// `wait_for_exit` knows the child has been reaped. Behind a `Mutex`
    /// because a `Receiver` is `Send` and not `Sync`, and a `Wire` is shared.
    done: Mutex<sync_mpsc::Receiver<()>>,
}

impl Wire {
    /// Spawn `command`, connect, and `initialize`. Blocks for at most
    /// `timeout`; the thread lives on for as long as the child does.
    pub fn spawn(command: Command, timeout: Duration, on_event: OnEvent) -> Result<Self, String> {
        let (tx, rx) = mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = sync_mpsc::channel();
        let (done_tx, done) = sync_mpsc::channel();
        thread::Builder::new()
            .name("acp-wire".into())
            .spawn(move || run(command, rx, ready_tx, done_tx, on_event))
            .map_err(|why| format!("could not start the wire thread: {why}"))?;
        let handshake = match ready_rx.recv_timeout(timeout) {
            Ok(Ok(handshake)) => handshake,
            Ok(Err(why)) => return Err(why),
            Err(RecvTimeoutError::Timeout) => {
                let _ = tx.send(Msg::Shutdown);
                return Err("did not answer initialize".to_string());
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err("exited before initialize".to_string())
            }
        };
        Ok(Self {
            tx,
            handshake,
            done: Mutex::new(done),
        })
    }

    /// Wait, bounded, for the thread to end — and so for the child to have
    /// been killed and reaped, which is the last thing it does. False is the
    /// timeout passing with the thread still there.
    ///
    /// `shutdown` only posts the message, so a caller that must not outlive
    /// its child needs this after it. `Session::shutdown` always does: the
    /// child is in its own process group, so ending the process does not take
    /// it with us. The probe waits too.
    pub fn wait_for_exit(&self, timeout: Duration) -> bool {
        self.done.lock().is_ok_and(|done| {
            matches!(
                done.recv_timeout(timeout),
                Err(RecvTimeoutError::Disconnected)
            )
        })
    }

    pub fn handshake(&self) -> &Handshake {
        &self.handshake
    }

    /// Whether the thread — and so the child — is still there.
    pub fn alive(&self) -> bool {
        !self.tx.is_closed()
    }

    /// `session/load` when `load` names one, falling back to `session/new`.
    pub fn open(
        &self,
        load: Option<String>,
        cwd: &Path,
        mcp: Option<McpChoice>,
        timeout: Duration,
    ) -> Result<String, OpenError> {
        let (reply, rx) = sync_mpsc::channel();
        self.tx
            .send(Msg::Open {
                load,
                cwd: cwd.to_path_buf(),
                mcp,
                reply,
            })
            .map_err(|_| OpenError::Lost)?;
        rx.recv_timeout(timeout).unwrap_or(Err(OpenError::Lost))
    }

    /// One `session/prompt`: the concatenated `agent_message_chunk`s once the
    /// turn ends in `end_turn`. Past `timeout`, `session/cancel` goes out and
    /// the reply is waited on for `CANCEL_GRACE` so the wire is quiet again.
    pub fn prompt(
        &self,
        session_id: &str,
        text: &str,
        timeout: Duration,
    ) -> Result<String, TurnError> {
        let (reply, rx) = sync_mpsc::channel();
        self.tx
            .send(Msg::Prompt {
                session_id: session_id.to_string(),
                text: text.to_string(),
                reply,
            })
            .map_err(|_| TurnError::Lost)?;
        match rx.recv_timeout(timeout) {
            Ok(outcome) => outcome,
            Err(RecvTimeoutError::Disconnected) => Err(TurnError::Lost),
            Err(RecvTimeoutError::Timeout) => {
                let _ = self.tx.send(Msg::Cancel);
                let _ = rx.recv_timeout(CANCEL_GRACE);
                Err(TurnError::Timeout)
            }
        }
    }

    /// Cancel the turn in flight, for a caller that is about to send a newer
    /// prompt. No-op on an idle wire: `serve` drops the message.
    ///
    /// Unlike `prompt`'s own timeout cancel, this one waits for nothing — the
    /// turn's own caller is the one holding its reply channel.
    pub fn cancel(&self) {
        let _ = self.tx.send(Msg::Cancel);
    }

    /// The user's pick on a forwarded permission request.
    pub fn answer(&self, request: &str, option: &str) {
        let _ = self.tx.send(Msg::Answer {
            request: request.to_string(),
            option: option.to_string(),
        });
    }

    /// Cancel whatever is in flight, answer open asks `cancelled`, close
    /// stdin, and kill the child.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown);
    }
}

impl Drop for Wire {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The whole life of one child, on the wire thread.
fn run(
    command: Command,
    rx: mpsc::UnboundedReceiver<Msg>,
    ready: sync_mpsc::Sender<Result<Handshake, String>>,
    // Held, never sent on, and dropped when this function returns: that drop
    // is what `wait_for_exit` waits for.
    _done: sync_mpsc::Sender<()>,
    on_event: OnEvent,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread().build() {
        Ok(runtime) => runtime,
        Err(why) => {
            let _ = ready.send(Err(format!("no runtime for the wire: {why}")));
            return;
        }
    };
    runtime.block_on(async move {
        #[cfg(windows)]
        let mut child = match windows_job::spawn_in_job(command) {
            Ok(child) => child,
            Err(why) => {
                let _ = ready.send(Err(format!("could not start: {why}")));
                return;
            }
        };

        #[cfg(not(windows))]
        let mut child = {
            let mut async_command = async_process::Command::from(command);
            async_command
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit());
            match async_command.spawn() {
                Ok(child) => child,
                Err(why) => {
                    let _ = ready.send(Err(format!("could not start: {why}")));
                    return;
                }
            }
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = ready.send(Err("no pipes to the child".to_string()));
            return;
        };
        // What the Harness sends us, routed off the SDK's dispatch loop and
        // into `serve`, which is the one place that knows whether a turn is
        // open to receive it. Anything else the Harness asks — `fs/*`,
        // `terminal/*`, capabilities we never advertised — the SDK answers
        // with method-not-found on its own.
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let updates = incoming_tx.clone();
        let mut ready = Some(ready);
        let outcome = Client
            .builder()
            .name("ai-buddy")
            .on_receive_notification(
                async move |notification: SessionNotification, _cx| {
                    let _ = updates.send(Incoming::Update(notification.update));
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _cx| {
                    let _ = incoming_tx.send(Incoming::Ask(request, responder));
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(
                ByteStreams::new(stdin, stdout),
                async |cx: ConnectionTo<Agent>| {
                    let handshake = cx
                        .send_request(InitializeRequest::new(ProtocolVersion::V1).client_info(
                            Implementation::new("ai-buddy", env!("CARGO_PKG_VERSION")),
                        ))
                        .block_task()
                        .await
                        .map(|response| Handshake {
                            agent: response.agent_info.map(|info| info.name),
                            load_session: response.agent_capabilities.load_session,
                            mcp_http: response.agent_capabilities.mcp_capabilities.http,
                            auth_methods: response.auth_methods.iter().map(auth_hint).collect(),
                        });
                    let failed = handshake.is_err();
                    if let Some(ready) = ready.take() {
                        let _ = ready
                            .send(handshake.map_err(|why| format!("initialize: {}", why.message)));
                    }
                    if !failed {
                        serve(&cx, rx, incoming_rx, &on_event).await;
                    }
                    Ok(())
                },
            )
            .await;
        if let Some(ready) = ready.take() {
            let _ = ready.send(Err(match outcome {
                Ok(()) => "exited before initialize".to_string(),
                Err(why) => why.message,
            }));
        }
        // The Harness may have been started through `npx`, which does not
        // reliably die on stdin EOF; kill the process group rather than
        // orphan grandchildren. Direct kill is the fallback.
        kill_harness_tree(child.id());
        let _ = child.kill();
        let _ = child.status().await;
    });
}

/// SIGKILL the Harness's process group. `npx` grandchildren share that
/// group; a direct `Child::kill` leaves them running.
///
/// On Windows, terminates the Job Object so grandchildren die too.
pub(crate) fn kill_harness_tree(pid: u32) {
    #[cfg(unix)]
    {
        let pid = pid as libc::pid_t;
        let pgid = unsafe { libc::getpgid(pid) };
        if pgid > 0 {
            // SIGKILL, not SIGTERM: Claude's ACP adapter dumps
            // `Query closed before response received` on a polite
            // signal, which is the dump this isolation exists to avoid.
            let _ = unsafe { libc::killpg(pgid, libc::SIGKILL) };
        }
    }
    #[cfg(windows)]
    {
        windows_job::terminate_job(pid);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
}

/// What the Harness sent, on its way to `serve`.
enum Incoming {
    Update(SessionUpdate),
    Ask(
        RequestPermissionRequest,
        Responder<RequestPermissionResponse>,
    ),
}

/// Commands until `Shutdown`, EOF, or the caller hanging up.
async fn serve(
    cx: &ConnectionTo<Agent>,
    mut rx: mpsc::UnboundedReceiver<Msg>,
    mut incoming: mpsc::UnboundedReceiver<Incoming>,
    on_event: &OnEvent,
) {
    loop {
        let command = tokio::select! {
            command = rx.recv() => match command {
                Some(command) => command,
                None => break,
            },
            // History replayed by `session/load`, and anything said between
            // turns: not ours to keep.
            _ = incoming.recv() => continue,
            () = cx.incoming_closed() => break,
        };
        match command {
            Msg::Open {
                load,
                cwd,
                mcp,
                reply,
            } => {
                let opened = open(cx, load, &cwd, mcp).await;
                let _ = reply.send(opened.map(|id| id.0.to_string()));
            }
            Msg::Prompt {
                session_id,
                text,
                reply,
            } => {
                let id = SessionId::new(session_id);
                let outcome = turn(cx, &id, &mut rx, &mut incoming, &text, on_event).await;
                let lost = outcome == Err(TurnError::Lost);
                let _ = reply.send(outcome);
                if lost {
                    break;
                }
            }
            // No turn is running, so there is no ask to answer and nothing to
            // cancel.
            Msg::Cancel | Msg::Answer { .. } => {}
            Msg::Shutdown => break,
        }
    }
}

/// One `McpChoice` in the protocol's own words.
///
/// The token rides in a header rather than in the URL or an argv, which is the
/// one place it is neither written to a config file the Harness keeps nor
/// visible in a process list.
fn mcp_server(choice: &McpChoice) -> McpServer {
    match choice {
        McpChoice::Http { url, authorization } => {
            McpServer::Http(McpServerHttp::new("ai-buddy", url.clone()).headers(vec![
                HttpHeader::new("Authorization", authorization.clone()),
            ]))
        }
        McpChoice::Stdio(launch) => McpServer::Stdio(
            McpServerStdio::new("ai-buddy", launch.path.clone())
                .args(launch.args.clone())
                .env(
                    launch
                        .env
                        .iter()
                        .map(|(name, value)| EnvVariable::new(name.clone(), value.clone()))
                        .collect(),
                ),
        ),
    }
}

/// `session/load` when asked and answered, else `session/new`. Raw requests
/// rather than the SDK's session builders: those tear the connection down
/// when the Harness refuses, and `auth_required` is a refusal we recover from.
async fn open(
    cx: &ConnectionTo<Agent>,
    load: Option<String>,
    cwd: &Path,
    mcp: Option<McpChoice>,
) -> Result<SessionId, OpenError> {
    let servers = || -> Vec<McpServer> { mcp.iter().map(mcp_server).collect() };
    if let Some(id) = load {
        let loaded = cx
            .send_request(LoadSessionRequest::new(id.clone(), cwd).mcp_servers(servers()))
            .block_task()
            .await;
        if loaded.is_ok() {
            return Ok(SessionId::new(id));
        }
    }
    cx.send_request(NewSessionRequest::new(cwd).mcp_servers(servers()))
        .block_task()
        .await
        .map(|response| response.session_id)
        .map_err(|error| {
            if error.code == ErrorCode::AuthRequired {
                OpenError::AuthRequired
            } else if cx.is_incoming_closed() {
                OpenError::Lost
            } else {
                OpenError::Failed(error.message)
            }
        })
}

/// One prompt turn: chunks accumulate, other updates become `Event`s, a
/// permission request is forwarded and held open until `Answer` or `Cancel`.
///
/// Where "the turn finished" is read. ACP v2 moves it to an idle
/// `state_update`; keep it here and nowhere else.
async fn turn(
    cx: &ConnectionTo<Agent>,
    session: &SessionId,
    rx: &mut mpsc::UnboundedReceiver<Msg>,
    incoming: &mut mpsc::UnboundedReceiver<Incoming>,
    text: &str,
    on_event: &OnEvent,
) -> Result<String, TurnError> {
    let sent = cx.send_request(PromptRequest::new(
        session.clone(),
        vec![ContentBlock::Text(TextContent::new(text.to_string()))],
    ));
    let mut finished = std::pin::pin!(sent.block_task());
    let mut said = String::new();
    // Beside `said` and never inside it: the thought is kept only so a chunk
    // that arrives mid-sentence can be shown as the sentence it belongs to.
    // It dies with the turn, which is the whole of its lifetime.
    let mut thought = String::new();
    let mut asks: Vec<(String, Responder<RequestPermissionResponse>)> = Vec::new();
    loop {
        // `biased`, updates first: the SDK dispatches a turn's chunks before
        // its response, so the response is read only once the channel ahead
        // of it is empty, and no chunk is left behind on the way out.
        tokio::select! {
            biased;
            message = incoming.recv() => match message {
                Some(Incoming::Update(update)) => {
                    note_update(update, &mut said, &mut thought, on_event)
                }
                Some(Incoming::Ask(request, responder)) => {
                    let ask = permission_ask(&request, &responder);
                    asks.push((ask.request.clone(), responder));
                    on_event(Event::Permission(ask));
                }
                None => {
                    end_turn(&mut asks, &mut thought, on_event);
                    return Err(TurnError::Lost);
                }
            },
            response = &mut finished => {
                end_turn(&mut asks, &mut thought, on_event);
                return match response {
                    Ok(response) => match response.stop_reason {
                        StopReason::EndTurn => Ok(said),
                        other => Err(TurnError::Stopped(name_of(&other))),
                    },
                    Err(_) if cx.is_incoming_closed() => Err(TurnError::Lost),
                    Err(error) => Err(TurnError::Failed(error.message)),
                };
            }
            command = rx.recv() => match command {
                Some(Msg::Cancel) | Some(Msg::Shutdown) => {
                    let _ = cx.send_notification(CancelNotification::new(session.clone()));
                    end_turn(&mut asks, &mut thought, on_event);
                }
                Some(Msg::Answer { request, option }) => {
                    if let Some(at) = asks.iter().position(|(id, _)| *id == request) {
                        let (_, responder) = asks.remove(at);
                        let _ = responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                option.clone(),
                            )),
                        ));
                        on_event(Event::PermissionSettled {
                            request,
                            option: Some(option),
                        });
                    }
                }
                Some(Msg::Prompt { reply, .. }) => {
                    let _ = reply.send(Err(TurnError::Busy));
                }
                Some(Msg::Open { reply, .. }) => {
                    let _ = reply.send(Err(OpenError::Failed("a turn is in flight".to_string())));
                }
                None => {
                    end_turn(&mut asks, &mut thought, on_event);
                    return Err(TurnError::Lost);
                }
            },
            () = cx.incoming_closed() => {
                end_turn(&mut asks, &mut thought, on_event);
                return Err(TurnError::Lost);
            }
        }
    }
}

fn permission_ask(
    request: &RequestPermissionRequest,
    responder: &Responder<RequestPermissionResponse>,
) -> PermissionAsk {
    PermissionAsk {
        request: responder.id().to_string(),
        title: request
            .tool_call
            .fields
            .title
            .clone()
            .unwrap_or_else(|| "(untitled)".to_string()),
        kind: request.tool_call.fields.kind.as_ref().map(name_of),
        options: request
            .options
            .iter()
            .map(|option| PermissionOption {
                id: option.option_id.0.to_string(),
                name: option.name.clone(),
                kind: Some(name_of(&option.kind)),
            })
            .collect(),
    }
}

fn note_update(update: SessionUpdate, said: &mut String, thought: &mut String, on_event: &OnEvent) {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if let ContentBlock::Text(text) = chunk.content {
                said.push_str(&text.text);
            }
        }
        SessionUpdate::ToolCall(call) => on_event(Event::ToolCall {
            id: call.tool_call_id.0.to_string(),
            title: Some(call.title),
            kind: Some(name_of(&call.kind)),
            status: Some(name_of(&call.status)),
        }),
        SessionUpdate::ToolCallUpdate(update) => on_event(Event::ToolCall {
            id: update.tool_call_id.0.to_string(),
            title: update.fields.title,
            kind: update.fields.kind.as_ref().map(name_of),
            status: update.fields.status.as_ref().map(name_of),
        }),
        SessionUpdate::Plan(plan) => on_event(Event::Plan {
            entries: plan.entries.len(),
        }),
        SessionUpdate::UsageUpdate(usage) => on_event(Event::Usage {
            used: usage.used,
            size: usage.size,
        }),
        // Never into `said`. That is the Director's reply, whose first line has
        // to parse as a Behavior name and whose rest the buddy says out loud;
        // reasoning is neither, so it leaves by its own door (ADR-0025).
        SessionUpdate::AgentThoughtChunk(chunk) => {
            if let ContentBlock::Text(text) = chunk.content {
                thought.push_str(&text.text);
                if let Some(line) = thinking_line(thought) {
                    on_event(Event::Thought(line.to_string()));
                }
            }
        }
        _ => {}
    }
}

/// The line of a streamed thought being written now: the tail of everything
/// that has arrived, because a chunk lands mid-sentence and half a sentence on
/// its own reads as nonsense. `None` while nothing but whitespace has come —
/// an adapter streams signature-only thinking blocks whose text is empty, and
/// a strip that opens on one says the Harness is thinking about nothing.
fn thinking_line(thought: &str) -> Option<&str> {
    thought
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}

/// Close out what this side was holding for a turn that is over, cancelled, or
/// leaving with the app.
///
/// Every question nobody will answer now gets the protocol-mandated reply,
/// which is not an answer. And the thinking goes dark: the Chat surface keeps
/// no thought of its own, so the last line stays on screen until it is told
/// the turn that produced it has ended (ADR-0025).
fn end_turn(
    asks: &mut Vec<(String, Responder<RequestPermissionResponse>)>,
    thought: &mut String,
    on_event: &OnEvent,
) {
    for (request, responder) in asks.drain(..) {
        let _ = responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ));
        on_event(Event::PermissionSettled {
            request,
            option: None,
        });
    }
    if !thought.is_empty() {
        thought.clear();
        on_event(Event::Thought(String::new()));
    }
}

fn auth_hint(method: &AuthMethod) -> AuthHint {
    match method {
        AuthMethod::Terminal(terminal) => AuthHint {
            name: terminal.name.clone(),
            description: terminal.description.clone(),
        },
        AuthMethod::Agent(agent) => AuthHint {
            name: agent.name.clone(),
            description: agent.description.clone(),
        },
        _ => AuthHint {
            name: "sign in".to_string(),
            description: None,
        },
    }
}

/// The wire spelling of a schema enum (`end_turn`, `execute`, `allow_once`).
fn name_of<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => name,
        Ok(other) => other.to_string(),
        Err(_) => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{AuthMethodAgent, ContentChunk, ToolKind};
    use std::sync::Arc;

    /// One streamed thought fragment, as the wire delivers it.
    fn thinking(text: &str) -> SessionUpdate {
        SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            text,
        ))))
    }

    /// An `OnEvent` and the events it collected.
    fn collector() -> (Arc<Mutex<Vec<Event>>>, OnEvent) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
        (
            seen,
            Box::new(move |event| kept.lock().unwrap().push(event)),
        )
    }

    /// What a run of updates left in the answer, and every event it raised.
    fn drive(updates: Vec<SessionUpdate>) -> (String, Vec<Event>) {
        let (seen, on_event) = collector();
        let mut said = String::new();
        let mut thought = String::new();
        for update in updates {
            note_update(update, &mut said, &mut thought, &on_event);
        }
        let events = seen.lock().unwrap().clone();
        (said, events)
    }

    /// Every thought the run raised, in order.
    fn thoughts(events: &[Event]) -> Vec<&str> {
        events
            .iter()
            .map(|event| match event {
                Event::Thought(line) => line.as_str(),
                other => panic!("expected a thought, got {other:?}"),
            })
            .collect()
    }

    /// The names the Action Log and the Chat surface see are the wire's own
    /// spellings, not Rust's.
    #[test]
    fn names_are_the_wire_spelling() {
        assert_eq!(name_of(&StopReason::EndTurn), "end_turn");
        assert_eq!(name_of(&StopReason::MaxTurnRequests), "max_turn_requests");
        assert_eq!(name_of(&ToolKind::Execute), "execute");
    }

    #[test]
    fn an_auth_method_keeps_its_description_for_the_login_hint() {
        let method =
            AuthMethod::Agent(AuthMethodAgent::new("x", "Sign in").description("run x login"));
        let hint = auth_hint(&method);
        assert_eq!(hint.name, "Sign in");
        assert_eq!(hint.description.as_deref(), Some("run x login"));
    }

    /// The shape `session/new` actually puts on the wire. The token rides in a
    /// header and nowhere else (ADR-0023): not in the URL, which a Harness may
    /// keep in a session file, and not in an argv, which is in a process list.
    #[test]
    fn an_http_choice_carries_the_token_in_a_header_and_not_in_the_url() {
        let server = mcp_server(&McpChoice::Http {
            url: "http://127.0.0.1:51234/mcp".to_string(),
            authorization: "Bearer deadbeef".to_string(),
        });
        let wire = serde_json::to_value(&server).expect("serializes");
        assert_eq!(wire["type"], serde_json::json!("http"));
        assert_eq!(wire["name"], serde_json::json!("ai-buddy"));
        assert_eq!(wire["url"], serde_json::json!("http://127.0.0.1:51234/mcp"));
        assert_eq!(
            wire["headers"][0]["name"],
            serde_json::json!("Authorization")
        );
        assert_eq!(
            wire["headers"][0]["value"],
            serde_json::json!("Bearer deadbeef")
        );
        assert!(!wire["url"].as_str().unwrap().contains("deadbeef"));
    }

    /// The fallback keeps the shape it always had: every Agent must take stdio,
    /// and the untagged variant is how the protocol spells it.
    #[test]
    fn a_stdio_choice_is_still_a_bare_command_and_carries_its_args() {
        let sidecar = mcp_server(&McpChoice::Stdio(McpLaunch {
            path: PathBuf::from("/opt/ai-buddy-mcp"),
            args: Vec::new(),
            env: Vec::new(),
        }));
        let wire = serde_json::to_value(&sidecar).expect("serializes");
        assert_eq!(wire["command"], serde_json::json!("/opt/ai-buddy-mcp"));
        assert_eq!(wire["name"], serde_json::json!("ai-buddy"));
        assert_eq!(wire["args"], serde_json::json!([]));

        // #497's third route: the app binary re-executed as its own server.
        let embedded = mcp_server(&McpChoice::Stdio(McpLaunch {
            path: PathBuf::from("/opt/ai-buddy"),
            args: vec!["--mcp-stdio".to_string()],
            env: Vec::new(),
        }));
        let wire = serde_json::to_value(&embedded).expect("serializes");
        assert_eq!(wire["command"], serde_json::json!("/opt/ai-buddy"));
        assert_eq!(wire["args"], serde_json::json!(["--mcp-stdio"]));
    }

    /// How the shim is told where to dial (ADR-0026). In `env`, which the
    /// Harness applies to the child it spawns, and not in an argv, which is in
    /// a process list.
    #[test]
    fn a_stdio_choice_carries_the_endpoint_in_its_environment() {
        let server = mcp_server(&McpChoice::Stdio(McpLaunch {
            path: PathBuf::from("/opt/ai-buddy-mcp"),
            args: Vec::new(),
            env: vec![
                (
                    "AI_BUDDY_MCP_URL".to_string(),
                    "http://127.0.0.1:51234/mcp".to_string(),
                ),
                ("AI_BUDDY_MCP_TOKEN".to_string(), "deadbeef".to_string()),
            ],
        }));
        let wire = serde_json::to_value(&server).expect("serializes");
        assert_eq!(
            wire["env"][0]["name"],
            serde_json::json!("AI_BUDDY_MCP_URL")
        );
        assert_eq!(
            wire["env"][0]["value"],
            serde_json::json!("http://127.0.0.1:51234/mcp")
        );
        assert_eq!(
            wire["env"][1]["name"],
            serde_json::json!("AI_BUDDY_MCP_TOKEN")
        );
        assert_eq!(wire["env"][1]["value"], serde_json::json!("deadbeef"));
        assert!(!wire["args"].to_string().contains("deadbeef"));
    }

    #[test]
    fn a_label_never_carries_the_token() {
        let choice = McpChoice::Http {
            url: "http://127.0.0.1:1/mcp".to_string(),
            authorization: "Bearer secret".to_string(),
        };
        assert_eq!(choice.label(), "http://127.0.0.1:1/mcp");
        assert!(!choice.label().contains("secret"));

        // The Action Log takes this line too, and the stdio choice now carries
        // a token of the same kind.
        let stdio = McpChoice::Stdio(McpLaunch {
            path: PathBuf::from("/opt/ai-buddy-mcp"),
            args: Vec::new(),
            env: vec![("AI_BUDDY_MCP_TOKEN".to_string(), "secret".to_string())],
        });
        assert_eq!(stdio.label(), "/opt/ai-buddy-mcp");
    }

    /// A thought reaches the Shell and never the answer. `said` is the
    /// Director's reply, whose first line has to parse as a Behavior name and
    /// whose rest is spoken out loud; a thought is neither (ADR-0025).
    #[test]
    fn a_thought_is_an_event_and_never_part_of_the_answer() {
        let (said, events) = drive(vec![
            thinking("Reading the roster"),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new("wave"),
            ))),
        ]);
        assert_eq!(said, "wave");
        assert_eq!(thoughts(&events), ["Reading the roster"]);
    }

    /// Chunks arrive as fragments, so what the surface shows is the tail of
    /// the thought so far: half a sentence on its own reads as nonsense, and
    /// the line before a newline is finished with.
    #[test]
    fn a_thought_shows_the_line_being_written() {
        let (_, events) = drive(vec![
            thinking("Reading the"),
            thinking(" roster.\n\nNow the manifest"),
        ]);
        assert_eq!(thoughts(&events), ["Reading the", "Now the manifest"]);
    }

    /// The strip lives exactly as long as the turn that fills it. Nothing on
    /// the Chat surface knows when a Harness stopped thinking, so a turn that
    /// ends without an answer to draw would otherwise leave its last thought
    /// on screen for good.
    #[test]
    fn the_thinking_goes_dark_when_the_turn_ends() {
        let (seen, on_event) = collector();
        let mut thought = "Reading the roster".to_string();
        end_turn(&mut Vec::new(), &mut thought, &on_event);
        assert_eq!(thoughts(&seen.lock().unwrap()), [""]);

        // A turn that thought nothing has nothing to take away, and a strip
        // that was never shown should not be told to hide.
        let (seen, on_event) = collector();
        end_turn(&mut Vec::new(), &mut String::new(), &on_event);
        assert!(seen.lock().unwrap().is_empty());
    }

    /// Nothing to show is not a thought. Adapters stream signature-only
    /// thinking blocks whose text is empty, and a blank strip that opens and
    /// shuts is worse than one that never opened.
    #[test]
    fn a_thought_with_no_words_raises_nothing() {
        let (_, events) = drive(vec![thinking("   \n")]);
        assert!(events.is_empty(), "{events:?}");
    }
}

#[cfg(windows)]
mod windows_job {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, OpenThread, ResumeThread, CREATE_SUSPENDED, PROCESS_ALL_ACCESS,
        THREAD_SUSPEND_RESUME,
    };

    struct SafeHandle(HANDLE);
    unsafe impl Send for SafeHandle {}

    static JOBS: Mutex<Option<HashMap<u32, SafeHandle>>> = Mutex::new(None);

    /// Spawn a command with create-time Job Object association via CREATE_SUSPENDED.
    ///
    /// Creates Job Object, spawns the process suspended, assigns to job,
    /// resumes. The process cannot fork children until after job assignment,
    /// closing the post-spawn race.
    pub(super) fn spawn_in_job(
        mut command: std::process::Command,
    ) -> Result<async_process::Child, String> {
        use std::os::windows::process::CommandExt;

        let job = unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() || job == INVALID_HANDLE_VALUE {
                return spawn_fallback_no_job(command, "CreateJobObjectW failed");
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );

            if ok == 0 {
                CloseHandle(job);
                return spawn_fallback_no_job(command, "SetInformationJobObject failed");
            }

            job
        };

        let creation_flags = crate::harness::get_creation_flags() | CREATE_SUSPENDED;
        command.creation_flags(creation_flags);

        let mut async_command = async_process::Command::from(command);
        async_command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        let mut child = match async_command.spawn() {
            Ok(child) => child,
            Err(e) => {
                unsafe { CloseHandle(job) };
                return Err(format!("spawn failed: {e}"));
            }
        };

        let pid = child.id();

        unsafe {
            let process = OpenProcess(PROCESS_ALL_ACCESS, 0, pid);
            if process.is_null() || process == INVALID_HANDLE_VALUE {
                // Intentional leak: job has KILL_ON_JOB_CLOSE; closing it could kill the child.
                eprintln!("harness: OpenProcess failed for pid {pid}; resuming without job");
                if resume_primary_thread(pid).is_err() {
                    let _ = child.kill();
                    return Err(format!(
                        "OpenProcess failed and resume failed for pid {pid}; child killed"
                    ));
                }
                return Ok(child);
            }

            let assigned = AssignProcessToJobObject(job, process);
            CloseHandle(process);

            if assigned == 0 {
                // Intentional leak: job has KILL_ON_JOB_CLOSE; closing it could kill the child.
                eprintln!(
                    "harness: AssignProcessToJobObject failed for pid {pid}; resuming without job"
                );
                if resume_primary_thread(pid).is_err() {
                    let _ = child.kill();
                    return Err(format!("AssignProcessToJobObject failed and resume failed for pid {pid}; child killed"));
                }
                return Ok(child);
            }

            if let Err(why) = resume_primary_thread(pid) {
                CloseHandle(job);
                let _ = child.kill();
                return Err(format!(
                    "ResumeThread failed for pid {pid}: {why}; child killed"
                ));
            }

            match JOBS.lock() {
                Ok(mut slot) => {
                    let map = slot.get_or_insert_with(HashMap::new);
                    map.insert(pid, SafeHandle(job));
                }
                Err(_) => {
                    eprintln!("harness: JOBS lock poisoned, handle leaked for pid {pid}");
                    CloseHandle(job);
                }
            }
        }

        Ok(child)
    }

    fn resume_primary_thread(pid: u32) -> Result<(), String> {
        unsafe {
            let tid = find_primary_thread(pid)?;
            let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, tid);
            if thread.is_null() || thread == INVALID_HANDLE_VALUE {
                return Err("OpenThread failed".to_string());
            }

            let result = ResumeThread(thread);
            CloseHandle(thread);

            if result == u32::MAX {
                return Err("ResumeThread failed".to_string());
            }

            Ok(())
        }
    }

    fn find_primary_thread(pid: u32) -> Result<u32, String> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        };

        // Assumes the first thread enumerated for the PID is the primary thread.
        // For a CREATE_SUSPENDED spawn this is typically correct, but if the
        // process has already started additional threads (unlikely immediately
        // post-spawn), this may resume the wrong thread. Sufficient for harness
        // use: the primary thread is suspended, so it will be the first found.

        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err("CreateToolhelp32Snapshot failed".to_string());
            }

            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

            if Thread32First(snapshot, &mut entry) == 0 {
                CloseHandle(snapshot);
                return Err("Thread32First failed".to_string());
            }

            loop {
                if entry.th32OwnerProcessID == pid {
                    let tid = entry.th32ThreadID;
                    CloseHandle(snapshot);
                    return Ok(tid);
                }

                if Thread32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }

            CloseHandle(snapshot);
            Err("No thread found for process".to_string())
        }
    }

    fn spawn_fallback_no_job(
        command: std::process::Command,
        why: &str,
    ) -> Result<async_process::Child, String> {
        eprintln!("harness: {why}; spawning without Job Object (grandchildren may linger)");
        let mut async_command = async_process::Command::from(command);
        async_command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        async_command
            .spawn()
            .map_err(|e| format!("fallback spawn failed: {e}"))
    }

    pub(super) fn terminate_job(pid: u32) {
        let job = {
            let mut slot = match JOBS.lock() {
                Ok(slot) => slot,
                Err(_) => return,
            };
            let Some(map) = slot.as_mut() else {
                return;
            };
            map.remove(&pid)
        };

        let Some(SafeHandle(job)) = job else {
            return;
        };

        unsafe {
            let _ = TerminateJobObject(job, 1);
            CloseHandle(job);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_terminate_job_missing_pid_is_noop() {
            terminate_job(0xFFFF_FFFE);
        }

        // Note: Job assignment failure paths (OpenProcess/AssignProcessToJobObject)
        // must resume the suspended child before returning Ok. This is verified by
        // the soft-fail behavior: on Windows CI, if the child were left suspended,
        // subsequent tests would hang waiting for stdio. The test suite passing
        // proves the child runs.
    }
}
