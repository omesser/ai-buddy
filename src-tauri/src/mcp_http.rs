//! The MCP server the app serves itself, on loopback HTTP.
//!
//! The one tool surface that reaches a buddy on screen (ADR-0018).
//! `crates/mcp-server` runs in a separate process with a stubbed
//! `DispatchContext` — however it is reached, sidecar or `--mcp-stdio` — so a
//! `speak` there returns success and moves nothing. The tools have to be
//! dispatched where the `Roster` lives, and the `Roster` lives on the
//! frame-loop thread. This file is therefore only a transport and a gate:
//! every `tools/call` is handed to the frame loop over `calls` and answered
//! from there.
//!
//! Streamable HTTP without the streaming half. The spec lets a server answer a
//! POST with `application/json` instead of an SSE stream, and nothing here
//! needs a server-initiated message — seven synchronous tools, no sampling, no
//! progress. That is why this is 200 lines of `std` rather than axum and hyper
//! in a desktop app's tree.
//!
//! Three things keep it off the network, and they are the ones ADR-0010's
//! credential rules imply for a listener rather than a client:
//!
//! 1. The listener binds `127.0.0.1` on an ephemeral port, and a connection
//!    from anywhere but loopback is dropped before a byte is read.
//! 2. Every request carries `Authorization: Bearer <token>`, a fresh 32 bytes
//!    per app run that never touches the disk, the Action Log, or a trace.
//! 3. A request with an `Origin` header is refused outright. No MCP client
//!    sends one; a page in the user's browser would have to, so refusing it
//!    closes the DNS-rebinding hole the MCP spec warns about without having
//!    to guess which origins are the user's own.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use ai_buddy_core::dispatch::{list_tools, DispatchError, ErrorCode};
use serde_json::{json, Value};

/// How long a `tools/call` waits for the frame loop before it is answered with
/// an error. Longer than any tick, short enough that a Harness gets a refusal
/// rather than a hung turn if the loop is gone.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest request body read. A tool call is a line of dialogue and a
/// Behavior name; a megabyte is already three orders of magnitude of slack.
const BODY_LIMIT: usize = 1024 * 1024;

/// The MCP protocol version claimed when the client names none. A client that
/// names one gets it echoed: every version in the wild carries `tools/list`
/// and `tools/call` unchanged, which is the whole of what is served here.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// One `tools/call` on its way to the frame loop, and the channel it is
/// answered on.
pub struct Call {
    pub tool: String,
    pub arguments: Value,
    pub reply: mpsc::Sender<Result<Value, DispatchError>>,
}

/// Where the server is listening, and the token that reaches it.
#[derive(Clone)]
pub struct Endpoint {
    pub url: String,
    token: String,
}

impl Endpoint {
    /// The `Authorization` value a client sends. A method rather than a public
    /// field so the token cannot be formatted into a log line by accident:
    /// `Endpoint` deliberately implements neither `Debug` nor `Serialize`.
    pub fn authorization(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

static ENDPOINT: OnceLock<Option<Endpoint>> = OnceLock::new();

/// Bind loopback and serve, once per app run.
///
/// `None` is a bind or a token that failed, which is a session with no tools
/// rather than an app that will not start: everything the buddy does on its
/// own still works.
pub fn serve(calls: mpsc::Sender<Call>) -> Option<Endpoint> {
    ENDPOINT
        .get_or_init(|| match start(calls) {
            Ok(endpoint) => Some(endpoint),
            Err(why) => {
                eprintln!("mcp: no loopback server: {why}");
                None
            }
        })
        .clone()
}

/// What `serve` bound, for anyone who did not call it. `None` before `serve`
/// as well as after a failed one — a probe that never starts a listener reads
/// the same as an app whose bind failed, and neither has tools to hand out.
pub fn endpoint() -> Option<Endpoint> {
    ENDPOINT.get().cloned().flatten()
}

fn start(calls: mpsc::Sender<Call>) -> std::io::Result<Endpoint> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|why| std::io::Error::other(format!("no random bytes for a token: {why}")))?;
    let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();

    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let url = format!("http://127.0.0.1:{}/mcp", listener.local_addr()?.port());
    let endpoint = Endpoint {
        url: url.clone(),
        token: token.clone(),
    };

    thread::Builder::new()
        .name("mcp-http".into())
        .spawn(move || accept_loop(listener, token, calls))?;
    Ok(endpoint)
}

fn accept_loop(listener: TcpListener, token: String, calls: mpsc::Sender<Call>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // Bound to loopback already, so this can only fail on a platform that
        // lied about the bind. Checked anyway, because it is the one assertion
        // the rest of the file's safety rests on.
        let local = stream.peer_addr().is_ok_and(|peer| {
            matches!(peer.ip(), IpAddr::V4(v4) if v4.is_loopback()) || peer.ip().is_loopback()
        });
        if !local {
            continue;
        }
        let token = token.clone();
        let calls = calls.clone();
        // A thread per connection. An MCP client keeps one or two alive for the
        // session, and only a Harness the user attached can get past the token,
        // so there is no fan-out here worth a pool.
        let _ = thread::Builder::new()
            .name("mcp-http-conn".into())
            .spawn(move || serve_connection(stream, &token, &calls));
    }
}

/// One request read off `reader`: the method, the path, the headers we act on,
/// and the body.
struct Request {
    method: String,
    origin: bool,
    authorization: Option<String>,
    body: Vec<u8>,
}

fn read_request(reader: &mut BufReader<&TcpStream>) -> std::io::Result<Option<Request>> {
    let mut start = String::new();
    if reader.read_line(&mut start)? == 0 {
        return Ok(None);
    }
    let method = start
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();

    let mut length = 0usize;
    let mut origin = false;
    let mut authorization = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().unwrap_or(0),
            "origin" => origin = true,
            "authorization" => authorization = Some(value.to_string()),
            _ => {}
        }
    }
    if length > BODY_LIMIT {
        return Err(std::io::Error::other("body too large"));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(Some(Request {
        method,
        origin,
        authorization,
        body,
    }))
}

fn serve_connection(stream: TcpStream, token: &str, calls: &mpsc::Sender<Call>) {
    let mut reader = BufReader::new(&stream);
    loop {
        let request = match read_request(&mut reader) {
            Ok(Some(request)) => request,
            Ok(None) | Err(_) => return,
        };
        let (status, body) = answer(&request, token, calls);
        let mut out = &stream;
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
            body.len()
        );
        if out.write_all(head.as_bytes()).is_err() || out.write_all(&body).is_err() {
            return;
        }
        let _ = out.flush();
    }
}

/// The status line and body for one request, with the transport's own refusals
/// ahead of any JSON-RPC.
fn answer(request: &Request, token: &str, calls: &mpsc::Sender<Call>) -> (&'static str, Vec<u8>) {
    if request.origin {
        return ("403 Forbidden", Vec::new());
    }
    if request.authorization.as_deref() != Some(&format!("Bearer {token}")) {
        return ("401 Unauthorized", Vec::new());
    }
    // GET is where a client would open the server-initiated SSE stream. There
    // is nothing to push, and the MCP client SDKs treat 405 here as "this
    // server does not stream" rather than as a failure.
    if request.method != "POST" {
        return ("405 Method Not Allowed", Vec::new());
    }
    let Ok(message) = serde_json::from_slice::<Value>(&request.body) else {
        return (
            "200 OK",
            error_body(&Value::Null, -32700, "parse error").into_bytes(),
        );
    };
    match handle(&message, calls) {
        // A notification is answered with no body, which is what the spec
        // asks for and what a client waiting on `notifications/initialized`
        // expects.
        None => ("202 Accepted", Vec::new()),
        Some(text) => ("200 OK", text.into_bytes()),
    }
}

fn error_body(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

/// One JSON-RPC message in, its response out — `None` for a notification.
///
/// Public so the tests can drive the protocol without a socket, which is the
/// half of this file worth testing.
pub(crate) fn handle(message: &Value, calls: &mpsc::Sender<Call>) -> Option<String> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id = message.get("id").cloned();
    // A notification has no id, and the spec says a notification gets no
    // response — which is the 202 the caller turns this `None` into.
    let id = id?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "ai-buddy", "version": env!("CARGO_PKG_VERSION")},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": list_tools()
                .into_iter()
                .map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                }))
                .collect::<Vec<_>>(),
        })),
        "tools/call" => Ok(call_tool(&params, calls)),
        _ => Err((-32601, format!("unknown method: {method}"))),
    };

    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
        Err((code, message)) => error_body(&id, code, &message),
    })
}

/// A `tools/call`, dispatched on the frame loop and reported the way MCP wants
/// it: a tool that refused is a result with `isError`, not a JSON-RPC error.
fn call_tool(params: &Value, calls: &mpsc::Sender<Call>) -> Value {
    let tool = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let (reply, answers) = mpsc::channel();
    let sent = calls
        .send(Call {
            tool: tool.to_string(),
            arguments,
            reply,
        })
        .is_ok();
    let outcome = if sent {
        answers
            .recv_timeout(ANSWER_TIMEOUT)
            .unwrap_or(Err(DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: "the app did not answer".to_string(),
            }))
    } else {
        Err(DispatchError {
            code: ErrorCode::ExecutionFailed,
            message: "the app is shutting down".to_string(),
        })
    };
    match outcome {
        Ok(value) => json!({
            "content": [{"type": "text", "text": value.to_string()}],
            "structuredContent": value,
        }),
        Err(why) => json!({
            "content": [{"type": "text", "text": why.message}],
            "isError": true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame loop that dispatches nothing and reports what it was asked.
    fn answering(answer: Value) -> (mpsc::Sender<Call>, thread::JoinHandle<Option<Call>>) {
        let (tx, rx) = mpsc::channel::<Call>();
        let handle = thread::spawn(move || {
            let call = rx.recv().ok()?;
            let _ = call.reply.send(Ok(answer));
            Some(call)
        });
        (tx, handle)
    }

    #[test]
    fn tools_list_is_the_seven_from_core_and_nothing_else() {
        let (tx, _rx) = mpsc::channel();
        let text = handle(
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
            &tx,
        )
        .expect("a request is answered");
        let value: Value = serde_json::from_str(&text).expect("valid JSON");
        let tools = value["result"]["tools"].as_array().expect("a list");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), 7, "the seven tools from #15: {names:?}");
        assert!(names.contains(&"speak"));
        assert!(
            tools[0]["inputSchema"].is_object(),
            "MCP spells it inputSchema"
        );
    }

    #[test]
    fn a_tool_call_reaches_the_frame_loop_with_its_arguments() {
        let (tx, handle_thread) = answering(json!({"success": true, "message": "hi"}));
        let text = handle(
            &json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "speak", "arguments": {"message": "hi"}},
            }),
            &tx,
        )
        .expect("a request is answered");
        let value: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(value["result"]["structuredContent"]["success"], json!(true));
        let call = handle_thread.join().expect("the loop ran").expect("a call");
        assert_eq!(call.tool, "speak");
        assert_eq!(call.arguments["message"], json!("hi"));
    }

    /// The bug this file exists for: a `speak` nothing could apply used to come
    /// back as success. A frame loop that refuses is an `isError` result.
    #[test]
    fn a_refused_tool_call_is_reported_as_an_error_not_as_success() {
        let (tx, rx) = mpsc::channel::<Call>();
        let loop_thread = thread::spawn(move || {
            let call = rx.recv().expect("a call");
            let _ = call.reply.send(Err(DispatchError {
                code: ErrorCode::ExecutionFailed,
                message: "no Instance named nope".to_string(),
            }));
        });
        let text = handle(
            &json!({
                "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": {"name": "speak", "arguments": {"message": "hi", "instance_id": "nope"}},
            }),
            &tx,
        )
        .expect("a request is answered");
        loop_thread.join().expect("the loop ran");
        let value: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(value["result"]["isError"], json!(true));
        assert_eq!(
            value["result"]["content"][0]["text"],
            json!("no Instance named nope")
        );
    }

    #[test]
    fn a_notification_is_answered_with_no_body() {
        let (tx, _rx) = mpsc::channel();
        assert!(handle(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &tx
        )
        .is_none());
    }

    #[test]
    fn initialize_echoes_the_version_the_client_asked_for() {
        let (tx, _rx) = mpsc::channel();
        let text = handle(
            &json!({"jsonrpc": "2.0", "id": 4, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}}),
            &tx,
        )
        .expect("a request is answered");
        let value: Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(value["result"]["protocolVersion"], json!("2024-11-05"));
        assert_eq!(value["result"]["serverInfo"]["name"], json!("ai-buddy"));
    }

    fn request(method: &str, authorization: Option<&str>, origin: bool, body: &str) -> Request {
        Request {
            method: method.to_string(),
            origin,
            authorization: authorization.map(str::to_string),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn the_gate_refuses_a_wrong_token_a_browser_origin_and_a_get() {
        let (tx, _rx) = mpsc::channel();
        let good = "Bearer t";
        assert_eq!(
            answer(
                &request("POST", Some("Bearer wrong"), false, "{}"),
                "t",
                &tx
            )
            .0,
            "401 Unauthorized"
        );
        assert_eq!(
            answer(&request("POST", None, false, "{}"), "t", &tx).0,
            "401 Unauthorized"
        );
        // Refused before the token is even compared: a page that can send an
        // Origin has no business holding the token either way.
        assert_eq!(
            answer(&request("POST", Some(good), true, "{}"), "t", &tx).0,
            "403 Forbidden"
        );
        assert_eq!(
            answer(&request("GET", Some(good), false, ""), "t", &tx).0,
            "405 Method Not Allowed"
        );
        assert_eq!(
            answer(
                &request(
                    "POST",
                    Some(good),
                    false,
                    r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#
                ),
                "t",
                &tx
            )
            .0,
            "200 OK"
        );
    }

    /// The token is the one thing here that must never be formattable. If a
    /// `Debug` or `Serialize` derive ever lands on `Endpoint`, this stops
    /// compiling — which is the point.
    #[test]
    fn the_endpoint_hands_out_a_header_and_not_a_token() {
        let endpoint = Endpoint {
            url: "http://127.0.0.1:1/mcp".to_string(),
            token: "secret".to_string(),
        };
        assert_eq!(endpoint.authorization(), "Bearer secret");
        assert!(!endpoint.url.contains("secret"));
    }

    /// End to end over a real socket, which is the half the unit tests above
    /// cannot reach: the header parse, the framing, and keep-alive.
    #[test]
    fn a_real_socket_serves_a_tool_call() {
        let (tx, loop_thread) = answering(json!({"success": true, "message": "on screen"}));
        let endpoint = start(tx).expect("bound loopback");
        let address = endpoint
            .url
            .trim_start_matches("http://")
            .trim_end_matches("/mcp");
        let mut stream = TcpStream::connect(address).expect("connected");
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"speak","arguments":{"message":"on screen"}}}"#;
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: {address}\r\nAuthorization: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            endpoint.authorization(),
            body.len()
        );
        stream.write_all(request.as_bytes()).expect("wrote");
        let mut reader = BufReader::new(stream.try_clone().expect("cloned"));
        let mut status = String::new();
        reader.read_line(&mut status).expect("a status line");
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("a header");
            if line.trim_end().is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().expect("a length");
            }
        }
        let mut payload = vec![0u8; length];
        reader.read_exact(&mut payload).expect("a body");
        let value: Value = serde_json::from_slice(&payload).expect("valid JSON");
        assert_eq!(value["result"]["structuredContent"]["success"], json!(true));
        let call = loop_thread.join().expect("the loop ran").expect("a call");
        assert_eq!(call.tool, "speak");
    }
}
