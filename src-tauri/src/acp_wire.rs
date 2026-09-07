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
    AuthMethod, CancelNotification, ContentBlock, ErrorCode, Implementation, InitializeRequest,
    LoadSessionRequest, McpServer, McpServerStdio, NewSessionRequest, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, SessionNotification, SessionUpdate, StopReason,
    TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo, Responder};
use serde::Serialize;
use tokio::sync::mpsc;

/// After `session/cancel`, how long the turn lock waits for the `cancelled`
/// reply before it is given back regardless.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

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
        mcp: Option<PathBuf>,
        reply: sync_mpsc::Sender<Result<String, OpenError>>,
    },
    Prompt {
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
    /// its child needs this after it. The app does not: `main.rs` answers the
    /// run event and the process ends, taking the thread with it. A probe a
    /// script waits on does.
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
        mcp: Option<PathBuf>,
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
    pub fn prompt(&self, text: &str, timeout: Duration) -> Result<String, TurnError> {
        let (reply, rx) = sync_mpsc::channel();
        self.tx
            .send(Msg::Prompt {
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
        let mut async_command = async_process::Command::from(command);
        async_command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        let mut child = match async_command.spawn() {
            Ok(child) => child,
            Err(why) => {
                let _ = ready.send(Err(format!("could not start: {why}")));
                return;
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
        // reliably die on stdin EOF; kill it rather than orphan it.
        let _ = child.kill();
        let _ = child.status().await;
    });
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
    let mut session: Option<SessionId> = None;
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
                if let Ok(id) = &opened {
                    session = Some(id.clone());
                }
                let _ = reply.send(opened.map(|id| id.0.to_string()));
            }
            Msg::Prompt { text, reply } => {
                let Some(id) = session.clone() else {
                    let _ = reply.send(Err(TurnError::Failed("no session open".to_string())));
                    continue;
                };
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

/// `session/load` when asked and answered, else `session/new`. Raw requests
/// rather than the SDK's session builders: those tear the connection down
/// when the Harness refuses, and `auth_required` is a refusal we recover from.
async fn open(
    cx: &ConnectionTo<Agent>,
    load: Option<String>,
    cwd: &Path,
    mcp: Option<PathBuf>,
) -> Result<SessionId, OpenError> {
    let servers = || -> Vec<McpServer> {
        mcp.iter()
            .map(|path| McpServer::Stdio(McpServerStdio::new("ai-buddy", path.clone())))
            .collect()
    };
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
    let mut asks: Vec<(String, Responder<RequestPermissionResponse>)> = Vec::new();
    loop {
        // `biased`, updates first: the SDK dispatches a turn's chunks before
        // its response, so the response is read only once the channel ahead
        // of it is empty, and no chunk is left behind on the way out.
        tokio::select! {
            biased;
            message = incoming.recv() => match message {
                Some(Incoming::Update(update)) => note_update(update, &mut said, on_event),
                Some(Incoming::Ask(request, responder)) => {
                    let ask = permission_ask(&request, &responder);
                    asks.push((ask.request.clone(), responder));
                    on_event(Event::Permission(ask));
                }
                None => {
                    cancel_asks(&mut asks);
                    return Err(TurnError::Lost);
                }
            },
            response = &mut finished => {
                cancel_asks(&mut asks);
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
                    cancel_asks(&mut asks);
                }
                Some(Msg::Answer { request, option }) => {
                    if let Some(at) = asks.iter().position(|(id, _)| *id == request) {
                        let (_, responder) = asks.remove(at);
                        let _ = responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option)),
                        ));
                    }
                }
                Some(Msg::Prompt { reply, .. }) => {
                    let _ = reply.send(Err(TurnError::Busy));
                }
                Some(Msg::Open { reply, .. }) => {
                    let _ = reply.send(Err(OpenError::Failed("a turn is in flight".to_string())));
                }
                None => {
                    cancel_asks(&mut asks);
                    return Err(TurnError::Lost);
                }
            },
            () = cx.incoming_closed() => {
                cancel_asks(&mut asks);
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

fn note_update(update: SessionUpdate, said: &mut String, on_event: &OnEvent) {
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
        _ => {}
    }
}

/// The protocol-mandated reply for a question nobody will answer now: the
/// turn is over, cancelled, or the app is leaving. Not an answer.
fn cancel_asks(asks: &mut Vec<(String, Responder<RequestPermissionResponse>)>) {
    for (_, responder) in asks.drain(..) {
        let _ = responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ));
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
    use agent_client_protocol::schema::v1::{AuthMethodAgent, ToolKind};

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
}
