//! The stdio MCP shim: it forwards to the running app and holds no state.
//!
//! **This process dispatches nothing.** ADR-0023 puts the tools where the
//! `Roster` is — inside the running app, which serves them on loopback HTTP —
//! and a Harness that advertises `mcpCapabilities.http` is handed that server
//! directly. Everything else lands here, reached three ways and always the
//! same code: `AI_BUDDY_MCP_BIN`, an `ai-buddy-mcp` sidecar beside the app, or
//! the app binary re-executed as `ai-buddy --mcp-stdio`. All three are a
//! process outside the running app, including the third, so all three relay
//! rather than answer. A Harness that does not advertise
//! `mcpCapabilities.http` on ACP `initialize` — `hermes` does not, though it
//! is an HTTP MCP client in its own right — therefore reaches the same
//! Instances as everyone else (ADR-0026), rather than stubs that report a line
//! was said while nothing appears on screen.
//!
//! Every message is relayed verbatim and the app's answer is passed back
//! untouched: this parses each line only far enough to report a failure
//! against it.
//!
//! Discovery and authorisation come from the environment the app puts on the
//! MCP server entry it hands the Harness — never from a file or a log, so
//! there is nothing on disk for another user to read and the token still dies
//! with the app run. The URL is refused unless it names this machine's
//! loopback: the token is an authorisation to move the buddy, and a variable
//! naming some other host would post it there.
//!
//! A Harness that hands a stdio server none of its environment (`cursor-agent`,
//! #1020) names the app's unix socket in `args` instead: `--sock <path>`. No
//! token travels on that transport; the socket's 0600 mode is what authorises.
//!
//! Anything that is not an answer — no variables, no app, a refused token, an
//! oversized or unreadable body — is a JSON-RPC error on a request and a line
//! on standard error on a notification. Reporting success while nothing moves
//! is the bug this shim exists to end.

use std::io::{BufRead, Write};
#[cfg(unix)]
use std::path::PathBuf;

use serde_json::{json, Value};

/// Where the app is listening, as `http://127.0.0.1:<port>/mcp`.
pub const URL_VAR: &str = "AI_BUDDY_MCP_URL";

/// The app's per-run bearer token, in the environment rather than a file.
pub const TOKEN_VAR: &str = "AI_BUDDY_MCP_TOKEN";

/// The argument that names the app's unix socket, for a Harness that keeps
/// its environment to itself. The path is not a secret, so it may sit in a
/// config file on disk.
pub const SOCK_FLAG: &str = "--sock";

/// How long the app has to answer before the Harness is told it did not.
/// Longer than the app's own wait for the frame loop, so its refusal wins the
/// race and says something more useful than a timeout here.
const ANSWER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The largest answer read back. A tool result is a line of dialogue or a
/// window list; a megabyte is orders of magnitude of slack, and past it the
/// call fails rather than relaying a body cut mid-JSON, which a Harness would
/// read as a malformed success.
const ANSWER_LIMIT: u64 = 1024 * 1024;

/// The only hosts the token may be posted to. The app binds `127.0.0.1`; the
/// other two are here so a hand-set variable during development is not a
/// puzzle.
const LOOPBACK: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// The running app's endpoint: where to POST and what reaches it.
///
/// Neither `Debug` nor `Serialize`, so the token cannot be formatted into a
/// log line by accident — the same guard the app puts on its own endpoint.
pub struct Endpoint {
    transport: Transport,
}

enum Transport {
    Http {
        url: String,
        authorization: String,
    },
    #[cfg(unix)]
    Unix(PathBuf),
}

impl Endpoint {
    /// Read the endpoint the app handed this process: `--sock <path>` in
    /// `args` first, else the two environment variables.
    pub fn discover(args: impl Iterator<Item = String>) -> Result<Self, String> {
        if let Some(path) = args.skip_while(|arg| arg != SOCK_FLAG).nth(1) {
            #[cfg(unix)]
            return Ok(Self {
                transport: Transport::Unix(PathBuf::from(path)),
            });
            #[cfg(not(unix))]
            {
                let _ = path;
                return Err(format!("{SOCK_FLAG}: no unix sockets on this platform"));
            }
        }
        let url = std::env::var(URL_VAR).map_err(|_| no_app(URL_VAR))?;
        let token = std::env::var(TOKEN_VAR).map_err(|_| no_app(TOKEN_VAR))?;
        Self::new(&url, &token)
    }

    fn new(url: &str, token: &str) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("{URL_VAR} is not an http:// URL"))?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        // `[::1]:8080` splits at the last colon, which is the port's.
        let host = authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host);
        if !LOOPBACK.contains(&host) {
            return Err(format!("{URL_VAR} does not name this machine"));
        }
        Ok(Self {
            transport: Transport::Http {
                url: url.to_string(),
                authorization: format!("Bearer {token}"),
            },
        })
    }

    /// POST one JSON-RPC message and return the app's body, empty where it
    /// answered a notification with nothing.
    fn post(&self, body: &str) -> Result<String, String> {
        match &self.transport {
            Transport::Http { url, authorization } => post_http(url, authorization, body),
            #[cfg(unix)]
            Transport::Unix(path) => post_unix(path, body),
        }
    }
}

fn post_http(url: &str, authorization: &str, body: &str) -> Result<String, String> {
    let mut response = ureq::post(url)
        .header("Authorization", authorization)
        .header("Content-Type", "application/json")
        .config()
        // A status is an answer to report, not a transport failure.
        .http_status_as_error(false)
        .timeout_global(Some(ANSWER_TIMEOUT))
        .build()
        .send(body)
        // `why` names the failure, not the endpoint: the Harness captures
        // this stream, and the port belongs in no log.
        .map_err(|why| format!("ai-buddy is not answering: {why}"))?;
    let code = response.status().as_u16();
    if !(200..300).contains(&code) {
        return Err(format!("ai-buddy refused the call: status {code}"));
    }
    response
        .body_mut()
        .with_config()
        .limit(ANSWER_LIMIT)
        .read_to_string()
        .map_err(|why| format!("ai-buddy's answer could not be read: {why}"))
}

/// One HTTP/1.1 POST by hand, because ureq dials TCP only. The same three
/// failures as `post_http`, worded the same, so a Harness log reads alike.
#[cfg(unix)]
fn post_unix(path: &std::path::Path, body: &str) -> Result<String, String> {
    use std::io::{BufReader, Read};
    use std::os::unix::net::UnixStream;

    let not_answering = |why: std::io::Error| format!("ai-buddy is not answering: {why}");
    let mut stream = UnixStream::connect(path).map_err(not_answering)?;
    stream
        .set_read_timeout(Some(ANSWER_TIMEOUT))
        .map_err(not_answering)?;
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: ai-buddy\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .map_err(not_answering)?;

    let unreadable = |why: String| format!("ai-buddy's answer could not be read: {why}");
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader
        .read_line(&mut status)
        .map_err(|why| unreadable(why.to_string()))?;
    let code: u16 = status
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| unreadable(format!("not an HTTP status line: {status:?}")))?;
    let mut length = 0u64;
    loop {
        let mut line = String::new();
        if reader
            .read_line(&mut line)
            .map_err(|why| unreadable(why.to_string()))?
            == 0
        {
            return Err(unreadable("closed before the headers ended".to_string()));
        }
        if line.trim_end().is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    if !(200..300).contains(&code) {
        return Err(format!("ai-buddy refused the call: status {code}"));
    }
    if length > ANSWER_LIMIT {
        return Err(unreadable(format!("{length} bytes is past the limit")));
    }
    let mut answer = String::new();
    reader
        .take(length)
        .read_to_string(&mut answer)
        .map_err(|why| unreadable(why.to_string()))?;
    Ok(answer)
}

fn no_app(var: &str) -> String {
    format!("{var} is unset: ai-buddy is not running, or this Harness dropped the environment it was given")
}

/// One line of stdio in, the line to write back out — `None` only for a
/// notification, which gets no response.
///
/// Every other outcome is answered, because a request left unanswered is a
/// Harness waiting forever. A failure is a JSON-RPC error; a notification that
/// failed is a line on standard error alone.
pub fn relay(line: &str, endpoint: Result<&Endpoint, &str>) -> Option<String> {
    let id = serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|message| message.get("id").cloned());
    let answered = match endpoint {
        Ok(endpoint) => endpoint.post(line),
        Err(why) => Err(why.to_string()),
    };
    let why = match answered {
        Ok(body) if !body.trim().is_empty() => return Some(body),
        // A notification is answered with no body, which is what the app's 202
        // means. A request answered with no body is a failure of this shim's
        // contract, not a success.
        Ok(_) if id.is_none() => return None,
        Ok(_) => "ai-buddy answered the request with nothing".to_string(),
        Err(why) => why,
    };
    eprintln!("ai-buddy-mcp: {why}");
    let id = id?;
    Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32603, "message": why}}).to_string())
}

/// Relay stdio to the app until the Harness hangs up.
///
/// `run` still lives here so `ai-buddy --mcp-stdio` and the `ai-buddy-mcp`
/// sidecar are one code path.
pub fn run() {
    let endpoint = Endpoint::discover(std::env::args().skip(1));
    if let Err(why) = &endpoint {
        eprintln!("ai-buddy-mcp: {why}");
    }
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Some(answer) = relay(&line, endpoint.as_ref().map_err(String::as_str)) else {
            continue;
        };
        if writeln!(stdout, "{answer}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Read};
    use std::net::TcpListener;
    use std::sync::{mpsc, Mutex};
    use std::thread;

    /// A stand-in for the app: answers one request, reporting the headers and
    /// body it was given.
    fn fake_app(status: &'static str, body: String) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback binds");
        let url = format!(
            "http://127.0.0.1:{}/mcp",
            listener.local_addr().unwrap().port()
        );
        let (sent, seen) = mpsc::channel();
        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a connection");
            answer_one(&stream, status, &body, &sent);
        });
        (url, seen)
    }

    /// The same stand-in on a unix socket, at a path nothing else uses.
    #[cfg(unix)]
    fn fake_app_on_socket(
        name: &str,
        status: &'static str,
        body: String,
    ) -> (std::path::PathBuf, mpsc::Receiver<String>) {
        let path =
            std::env::temp_dir().join(format!("ai-buddy-mcp-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("socket binds");
        let (sent, seen) = mpsc::channel();
        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("a connection");
            answer_one(&stream, status, &body, &sent);
        });
        (path, seen)
    }

    fn answer_one(
        mut stream: impl Read + Write,
        status: &str,
        body: &str,
        sent: &mpsc::Sender<String>,
    ) {
        let mut reader = BufReader::new(&mut stream);
        let mut request = String::new();
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
            let end = line.trim().is_empty();
            request.push_str(&line);
            if end {
                break;
            }
        }
        let mut payload = vec![0u8; length];
        reader.read_exact(&mut payload).expect("the body");
        request.push_str(&String::from_utf8_lossy(&payload));
        let _ = sent.send(request);
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
    }

    /// Run `body` with the two endpoint variables exported, and restore them
    /// after. Concurrent setenv/getenv is undefined behaviour and these are
    /// process-global, so mutation is serialised — the same shape
    /// `model::tests::with_env` uses in the shell.
    fn with_endpoint_env(url: Option<&str>, token: Option<&str>, body: impl FnOnce()) {
        static ENV: Mutex<()> = Mutex::new(());
        let _lock = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for (var, value) in [(URL_VAR, url), (TOKEN_VAR, token)] {
            match value {
                Some(value) => std::env::set_var(var, value),
                None => std::env::remove_var(var),
            }
        }
        body();
        for var in [URL_VAR, TOKEN_VAR] {
            std::env::remove_var(var);
        }
    }

    fn error_in(answer: &str) -> String {
        let answer: Value = serde_json::from_str(answer).expect("JSON-RPC");
        assert!(answer.get("result").is_none(), "reported success: {answer}");
        assert_eq!(answer["error"]["code"], json!(-32603));
        answer["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn a_shim_with_no_app_to_dial_answers_an_error_not_a_success() {
        // A port nothing is listening on: bound, then dropped.
        let dead = TcpListener::bind(("127.0.0.1", 0)).expect("loopback binds");
        let port = dead.local_addr().unwrap().port();
        drop(dead);
        let endpoint = Endpoint::new(&format!("http://127.0.0.1:{port}/mcp"), "token")
            .expect("a loopback URL");

        let answer = relay(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            Ok(&endpoint),
        )
        .expect("a request is answered");
        assert_eq!(
            serde_json::from_str::<Value>(&answer).unwrap()["id"],
            json!(1)
        );
        let why = error_in(&answer);
        // The same line goes to standard error, which the Harness captures.
        assert!(
            !why.contains(&port.to_string()),
            "the port reached a log: {why}"
        );
    }

    #[test]
    fn a_shim_with_no_endpoint_in_the_environment_answers_an_error() {
        let answer = relay(
            r#"{"jsonrpc":"2.0","id":"a","method":"tools/call"}"#,
            Err("AI_BUDDY_MCP_URL is unset"),
        )
        .expect("a request is answered");
        error_in(&answer);
        assert_eq!(
            serde_json::from_str::<Value>(&answer).unwrap()["id"],
            json!("a")
        );
    }

    #[test]
    fn a_failed_notification_is_reported_on_stderr_and_answered_with_nothing() {
        let answer = relay(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            Err("AI_BUDDY_MCP_URL is unset"),
        );
        assert!(answer.is_none(), "answered a notification: {answer:?}");
    }

    #[test]
    fn the_environment_discovers_the_app_and_the_token_travels_in_a_header() {
        let (url, seen) = fake_app(
            "200 OK",
            r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#.to_string(),
        );
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

        // The two variables the app sets on the MCP server entry it hands the
        // Harness.
        with_endpoint_env(Some(&url), Some("secret-token"), || {
            let endpoint =
                Endpoint::discover(std::iter::empty()).expect("the environment names an app");
            let answer = relay(call, Ok(&endpoint)).expect("an answer");
            assert_eq!(answer, r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#);
        });

        let request = seen
            .recv_timeout(ANSWER_TIMEOUT)
            .expect("the app saw a request");
        assert!(
            request.contains("authorization: Bearer secret-token"),
            "no bearer token in: {request}"
        );
        assert!(
            request.contains("POST /mcp HTTP/1.1"),
            "wrong target: {request}"
        );
        assert!(request.ends_with(call), "body was not relayed: {request}");
    }

    #[test]
    fn an_environment_naming_no_app_is_an_error_rather_than_a_default() {
        with_endpoint_env(None, None, || {
            assert!(
                Endpoint::discover(std::iter::empty()).is_err(),
                "invented an endpoint"
            );
        });
    }

    #[test]
    fn a_refused_token_is_an_error_rather_than_a_silent_success() {
        let (url, _seen) = fake_app("401 Unauthorized", String::new());
        let endpoint = Endpoint::new(&url, "stale-token").expect("a loopback URL");

        let answer = relay(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            Ok(&endpoint),
        )
        .expect("a request is answered");
        assert!(error_in(&answer).contains("401"));
    }

    #[test]
    fn an_answer_past_the_limit_is_an_error_rather_than_a_truncated_success() {
        let oversized = format!(
            r#"{{"jsonrpc":"2.0","id":3,"result":{{"text":"{}"}}}}"#,
            "x".repeat(ANSWER_LIMIT as usize)
        );
        let (url, _seen) = fake_app("200 OK", oversized);
        let endpoint = Endpoint::new(&url, "token").expect("a loopback URL");

        let answer = relay(
            r#"{"jsonrpc":"2.0","id":3,"method":"describe_screen"}"#,
            Ok(&endpoint),
        )
        .expect("a request is answered");
        error_in(&answer);
    }

    #[test]
    fn a_request_the_app_answers_with_nothing_is_an_error_not_silence() {
        let (url, _seen) = fake_app("200 OK", String::new());
        let endpoint = Endpoint::new(&url, "token").expect("a loopback URL");

        let answer = relay(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/list"}"#,
            Ok(&endpoint),
        )
        .expect("a request is answered");
        error_in(&answer);
    }

    #[test]
    fn an_accepted_notification_is_answered_with_nothing() {
        let (url, _seen) = fake_app("202 Accepted", String::new());
        let endpoint = Endpoint::new(&url, "token").expect("a loopback URL");

        let answer = relay(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            Ok(&endpoint),
        );
        assert!(answer.is_none(), "answered a notification: {answer:?}");
    }

    #[test]
    fn a_url_that_is_not_loopback_http_is_refused() {
        assert!(
            Endpoint::new("http://evil.example.com/mcp", "token").is_err(),
            "posted the token off this machine"
        );
        assert!(
            Endpoint::new("http://127.0.0.1.evil.example.com:80/mcp", "token").is_err(),
            "a host that merely starts with the loopback one"
        );
        assert!(Endpoint::new("https://example.com/mcp", "token").is_err());
        assert!(Endpoint::new("http://127.0.0.1:9/mcp", "token").is_ok());
        assert!(Endpoint::new("http://[::1]:9/mcp", "token").is_ok());
    }

    /// `--sock <path>` in args wins over the environment, and nothing in the
    /// request names a token: the socket's file mode is the authorization.
    #[cfg(unix)]
    #[test]
    fn a_sock_flag_dials_the_socket_and_sends_no_token() {
        let (path, seen) = fake_app_on_socket(
            "live",
            "200 OK",
            r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#.to_string(),
        );
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let args = [SOCK_FLAG.to_string(), path.display().to_string()];
        let endpoint = Endpoint::discover(args.into_iter()).expect("the flag names a socket");
        let answer = relay(call, Ok(&endpoint)).expect("an answer");
        assert_eq!(answer, r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#);

        let request = seen
            .recv_timeout(ANSWER_TIMEOUT)
            .expect("the app saw a request");
        assert!(
            request.starts_with("POST /mcp HTTP/1.1\r\n"),
            "wrong request line: {request}"
        );
        assert!(
            !request.to_ascii_lowercase().contains("authorization:"),
            "a token header on the socket: {request}"
        );
        assert!(request.ends_with(call), "body was not relayed: {request}");
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn a_socket_nobody_listens_on_answers_an_error_not_a_success() {
        let path =
            std::env::temp_dir().join(format!("ai-buddy-mcp-dead-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let args = [SOCK_FLAG.to_string(), path.display().to_string()];
        let endpoint = Endpoint::discover(args.into_iter()).expect("the flag names a socket");
        let answer = relay(
            r#"{"jsonrpc":"2.0","id":5,"method":"initialize"}"#,
            Ok(&endpoint),
        )
        .expect("a request is answered");
        assert_eq!(
            serde_json::from_str::<Value>(&answer).unwrap()["id"],
            json!(5)
        );
        error_in(&answer);
    }
}
