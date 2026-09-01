//! Completer stand-in, env config, and the in-flight session call.
//!
//! ponytail: HTTP chat-completions until #16 attaches a Harness. The
//! `Completer` trait is the seam; this file is the disposable impl. ADR-0008.
//!
//! The Completer runs on a worker thread. The frame loop only `try_recv`s.
//! #18 binds these settings. Until then they come from the env.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ai_buddy_core::director::{Completer, Context, ModelDirector, Pace, Wake, WAKE_EVERY};
use serde::Serialize;

/// Completer timeout. After this, fall back to `StaticDirector`.
///
/// Longer than a snappy chat-completions hop: xAI's Responses path can
/// think, and 8s was enough to lose a Grok wake to Static.
pub const TIMEOUT: Duration = Duration::from_secs(20);

const TRACE: &str = "AI_BUDDY_TRACE_DIRECTOR";

/// Prompt, raw reply, and parse. Off unless asked: a Character Prompt is
/// a paragraph, and printing it sixty times a minute would bury everything
/// else. Same gate as the hit-test and frame traces.
pub fn tracing() -> bool {
    crate::env_util::env_flag_is_on(TRACE)
}

fn trace_block(which: &str, text: &str) {
    eprintln!("director: --- {which} ---");
    eprint!("{text}");
    if !text.ends_with('\n') {
        eprintln!();
    }
    eprintln!("director: --- end {which} ---");
}

const API_KEY: &str = "AI_BUDDY_DIRECTOR_API_KEY";
const BASE_URL: &str = "AI_BUDDY_DIRECTOR_BASE_URL";
const MODEL: &str = "AI_BUDDY_DIRECTOR_MODEL";
const ENABLED: &str = "AI_BUDDY_DIRECTOR";
/// First ambient session wait, in seconds. Not a heartbeat.
const WAKE_SECS: &str = "AI_BUDDY_DIRECTOR_WAKE_SECS";

const DEFAULT_BASE: &str = "https://api.openai.com";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// Last Character Prompt and the config that produced it. #18 displays this.
#[derive(Clone, Debug, Serialize)]
pub struct DirectorInspect {
    pub enabled: bool,
    pub configured: bool,
    pub wake_secs: u64,
    pub last_payload: Option<String>,
}

/// Director on/off and the first ambient session wait. Read from the env.
#[derive(Clone, Debug)]
pub struct DirectorConfig {
    pub enabled: bool,
    pub configured: bool,
    /// The env var is set, but trim left nothing usable — `$XAI_API_KEY`
    /// expanding to empty used to look like the key was never offered.
    pub key_invalid: bool,
    /// Static Director interval. Free, so it stays short.
    pub wake_every: Duration,
    /// First ambient session wait. `Pace` doubles from here.
    pub ambient_first: Duration,
}

impl DirectorConfig {
    pub fn inspect(&self) -> DirectorInspect {
        DirectorInspect {
            enabled: self.enabled,
            configured: self.configured,
            wake_secs: self.ambient_first.as_secs(),
            last_payload: None,
        }
    }
}

/// What `AI_BUDDY_DIRECTOR_API_KEY` held, after quotes and whitespace.
#[derive(Clone, Debug, PartialEq, Eq)]
enum KeyRead {
    Unset,
    Invalid,
    Present(String),
}

/// Read Director config from the env. No API key means `StaticDirector` only.
pub fn config() -> DirectorConfig {
    let key = key_from_env();
    let configured = matches!(key, KeyRead::Present(_));
    let key_invalid = matches!(key, KeyRead::Invalid);
    let enabled = configured && !off();
    DirectorConfig {
        enabled,
        configured,
        key_invalid,
        wake_every: WAKE_EVERY,
        ambient_first: wake_secs().unwrap_or(Pace::FIRST),
    }
}

/// One line for the mode, and a warning when a key was offered but unusable.
///
/// Unset and empty used to be silent Static. The empty case is almost always
/// a `$VAR` that expanded to nothing, which is a mistake, not a choice.
pub fn startup_lines(config: &DirectorConfig) -> Vec<String> {
    let mut lines = Vec::new();
    if config.key_invalid {
        lines.push(format!(
            "director: warning: {API_KEY} is set but not a usable key; using StaticDirector"
        ));
    }
    if config.enabled {
        lines.push(format!(
            "director: model, ambient first {}s",
            config.ambient_first.as_secs()
        ));
    } else if config.configured {
        lines.push("director: off; using StaticDirector".to_string());
    } else {
        lines.push("director: StaticDirector".to_string());
    }
    lines
}

/// Strip wrapping quotes and whitespace. `.env` files quote keys; a
/// trailing newline is enough to 401 a Bearer token.
fn trim_key(raw: &str) -> Option<String> {
    let key = raw
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'')
        .to_string();
    (!key.is_empty()).then_some(key)
}

fn key_from_raw(raw: Option<&str>) -> KeyRead {
    match raw {
        None => KeyRead::Unset,
        Some(value) => match trim_key(value) {
            Some(key) => KeyRead::Present(key),
            None => KeyRead::Invalid,
        },
    }
}

fn key_from_env() -> KeyRead {
    match std::env::var(API_KEY) {
        Err(std::env::VarError::NotPresent) => KeyRead::Unset,
        Err(std::env::VarError::NotUnicode(_)) => KeyRead::Invalid,
        Ok(raw) => key_from_raw(Some(&raw)),
    }
}

fn secret_key() -> Option<String> {
    match key_from_env() {
        KeyRead::Present(key) => Some(key),
        KeyRead::Unset | KeyRead::Invalid => None,
    }
}

fn off() -> bool {
    matches!(
        std::env::var(ENABLED).ok().as_deref(),
        Some("off" | "0" | "false")
    )
}

fn wake_secs() -> Option<Duration> {
    let raw = std::env::var(WAKE_SECS).ok()?;
    let secs: u64 = raw.parse().ok()?;
    (secs > 0).then_some(Duration::from_secs(secs))
}

/// An OpenAI-compatible chat Completer, or `None` when no key is set.
pub fn endpoint() -> Option<Endpoint> {
    let api_key = secret_key()?;
    let base = std::env::var(BASE_URL).unwrap_or_else(|_| DEFAULT_BASE.to_string());
    let model = std::env::var(MODEL).unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    Some(Endpoint {
        api_key,
        url: completions_url(&base),
        model,
        timeout: TIMEOUT,
        session: Mutex::new(Vec::new()),
    })
}

/// Join a provider base onto the inference path without doubling `/v1`.
///
/// OpenAI, Anthropic's compatibility layer, and Ollama speak
/// `/v1/chat/completions`. xAI's current path is `/v1/responses`
/// ([docs](https://docs.x.ai/developers/model-capabilities/text/comparison));
/// chat-completions there is legacy. An explicit full path wins.
fn completions_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/chat/completions") || base.ends_with("/responses") {
        return base.to_string();
    }
    let path = if host_is_xai(base) {
        "responses"
    } else {
        "chat/completions"
    };
    if base.ends_with("/v1") {
        format!("{base}/{path}")
    } else {
        format!("{base}/v1/{path}")
    }
}

fn host_is_xai(url: &str) -> bool {
    url.split("://").nth(1).is_some_and(|rest| {
        rest.split('/')
            .next()
            .is_some_and(|host| host == "api.x.ai" || host.ends_with(".api.x.ai"))
    })
}

fn uses_responses(url: &str) -> bool {
    url.contains("/responses")
}

#[derive(Clone)]
struct Message {
    role: &'static str,
    content: String,
}

pub struct Endpoint {
    api_key: String,
    url: String,
    model: String,
    timeout: Duration,
    /// Opening + replies, so a follow-up can be short. ADR-0008.
    session: Mutex<Vec<Message>>,
}

impl Endpoint {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn is_xai(&self) -> bool {
        host_is_xai(&self.url)
    }

    /// Length and last four. Enough to tell two keys apart, not enough to use.
    pub fn key_fingerprint(&self) -> String {
        let n = self.api_key.len();
        let last = if n >= 4 {
            &self.api_key[n - 4..]
        } else {
            "****"
        };
        format!("len={n} last={last}")
    }

    pub fn origin(&self) -> String {
        origin(&self.url)
    }

    /// The other xAI inference path, if this URL has one.
    ///
    /// Keys are granted per-endpoint. `/v1/responses` is current; many console
    /// keys only have the legacy chat-completions ACL, which is a 403 rather
    /// than a 400. The probe hits both; `complete` retries the other on 403/404.
    pub fn alternate_url(&self) -> Option<String> {
        alternate_url(&self.url)
    }

    /// GET `url`. Non-2xx is still `Ok` — the status and body are the answer.
    pub fn get(&self, url: &str) -> Result<(u16, String), String> {
        let request = self.headers(ureq::get(url));
        match request.call() {
            Ok(response) => read_ok(response),
            Err(ureq::Error::Status(code, response)) => read_status(code, response),
            Err(error) => Err(error.to_string()),
        }
    }

    /// POST `url` with the body that path expects. Used by the probe so a
    /// 403 on `/v1/responses` is visible next to a 200 on chat-completions.
    pub fn post(&self, url: &str, prompt: &str) -> Result<String, String> {
        let snapshot = {
            let mut session = self.session.lock().expect("session lock");
            session.push(Message {
                role: "user",
                content: prompt.to_string(),
            });
            session.clone()
        };
        let body = request_body(&self.model, &snapshot, uses_responses(url));
        let request = self
            .headers(ureq::post(url))
            .set("Content-Type", "application/json");
        let text = match request.send_json(body) {
            Ok(response) => {
                let (_, text) = read_ok(response)?;
                text
            }
            Err(ureq::Error::Status(code, response)) => {
                self.session.lock().expect("session lock").pop();
                let (_, text) = read_status(code, response)?;
                return Err(status_error(url, code, &text));
            }
            Err(error) => {
                self.session.lock().expect("session lock").pop();
                return Err(error.to_string());
            }
        };
        match content_from_body(&text) {
            Ok(content) => {
                self.session.lock().expect("session lock").push(Message {
                    role: "assistant",
                    content: content.clone(),
                });
                Ok(content)
            }
            Err(error) => {
                self.session.lock().expect("session lock").pop();
                Err(format!("{url}: {error}"))
            }
        }
    }

    fn headers(&self, mut request: ureq::Request) -> ureq::Request {
        // ureq's default UA gets a WAF 403 on some edges; name ourselves.
        request = request
            .set("User-Agent", "ai-buddy")
            .set("Accept", "application/json")
            .timeout(self.timeout);
        if !self.api_key.is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", self.api_key));
        }
        // Anthropic's OpenAI layer accepts Bearer; the native Messages path
        // wants these two. Sending both covers either.
        if self.url.contains("api.anthropic.com") {
            request = request.set("anthropic-version", "2023-06-01");
            if !self.api_key.is_empty() {
                request = request.set("x-api-key", &self.api_key);
            }
        }
        request
    }
}

impl Completer for Endpoint {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        if tracing() {
            eprintln!("director: sending POST {} model={}", self.url, self.model);
            trace_block("prompt", prompt);
            eprintln!("director: waiting for model");
        }
        match self.post(&self.url, prompt) {
            Ok(content) => {
                if tracing() {
                    trace_block("model", &content);
                }
                Ok(content)
            }
            Err(error) => {
                if tracing() {
                    eprintln!("director: http {error}");
                }
                if let Some(alt) = fallback_url(&self.url, &error) {
                    if tracing() {
                        eprintln!("director: trying {alt}");
                    }
                    match self.post(&alt, prompt) {
                        Ok(content) => {
                            if tracing() {
                                trace_block("model", &content);
                            }
                            Ok(content)
                        }
                        Err(alt_error) => {
                            if tracing() {
                                eprintln!("director: http {alt_error}");
                            }
                            Err(alt_error)
                        }
                    }
                } else {
                    Err(error)
                }
            }
        }
    }
}

fn origin(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let host = rest.split('/').next().unwrap_or(rest);
    format!("{scheme}://{host}")
}

fn alternate_url(url: &str) -> Option<String> {
    if !host_is_xai(url) {
        return None;
    }
    if uses_responses(url) {
        Some(url.replacen("/responses", "/chat/completions", 1))
    } else if url.contains("/chat/completions") {
        Some(url.replacen("/chat/completions", "/responses", 1))
    } else {
        None
    }
}

/// Retry the legacy xAI path only when Responses was refused, not when the
/// body was wrong (400) or the key was unknown (401).
fn fallback_url(url: &str, error: &str) -> Option<String> {
    let refused = error.contains("status 403") || error.contains("status 404");
    (refused && uses_responses(url)).then(|| url.replacen("/responses", "/chat/completions", 1))
}

fn status_error(url: &str, code: u16, body: &str) -> String {
    const CAP: usize = 400;
    let body = body.trim();
    if body.is_empty() {
        format!("{url}: status {code}")
    } else if body.len() > CAP {
        format!("{url}: status {code} {}…", &body[..CAP])
    } else {
        format!("{url}: status {code} {body}")
    }
}

fn read_ok(response: ureq::Response) -> Result<(u16, String), String> {
    let code = response.status();
    let text = response.into_string().map_err(|error| error.to_string())?;
    Ok((code, text))
}

fn read_status(code: u16, response: ureq::Response) -> Result<(u16, String), String> {
    let text = response.into_string().unwrap_or_default();
    Ok((code, text))
}

/// Truncate a provider body for a terminal. The probe prints these; a WAF
/// HTML page should not scroll the useful lines off the screen.
fn clip_body(body: &str) -> String {
    const CAP: usize = 400;
    let body = body.trim();
    if body.len() > CAP {
        format!("{}…", &body[..CAP])
    } else {
        body.to_string()
    }
}

const PING: &str = "Reply with the single word pong and nothing else.";

/// Same Completer the overlay uses, without starting the overlay.
///
/// `scripts/probe-model.sh` is the face of this. Later a Harness attach
/// (#16) can share the command: same env, same exit codes, a second hop.
pub fn run_probe() -> i32 {
    let Some(endpoint) = endpoint() else {
        eprintln!("probe-model: no AI_BUDDY_DIRECTOR_API_KEY");
        return 2;
    };

    println!("probe-model");
    println!("  url    {}", endpoint.url());
    println!("  model  {}", endpoint.model());
    println!("  key    {}", endpoint.key_fingerprint());
    println!();

    let origin = endpoint.origin();
    probe_get(&endpoint, &format!("{origin}/v1/models"));
    if endpoint.is_xai() {
        probe_get(&endpoint, &format!("{origin}/v1/api-key"));
    }

    let mut ok = probe_post(&endpoint, endpoint.url());
    if let Some(alt) = endpoint.alternate_url() {
        ok = probe_post(&endpoint, &alt) || ok;
    }

    if ok {
        0
    } else {
        if endpoint.is_xai() {
            eprintln!(
                "The body above is the answer. 401 is a bad Bearer. \
                 403 is credits, a key ACL, or team mTLS."
            );
        }
        1
    }
}

fn probe_get(endpoint: &Endpoint, url: &str) {
    println!("GET {url}");
    match endpoint.get(url) {
        Ok((code, body)) => println!("  {code} {}", clip_body(&body)),
        Err(error) => println!("  transport {error}"),
    }
    println!();
}

fn probe_post(endpoint: &Endpoint, url: &str) -> bool {
    println!("POST {url}");
    match endpoint.post(url, PING) {
        Ok(text) => {
            println!("  ok {}", clip_body(&text));
            println!();
            true
        }
        Err(error) => {
            println!("  {}", clip_body(&error));
            println!();
            false
        }
    }
}

fn request_body(model: &str, session: &[Message], responses: bool) -> serde_json::Value {
    let input = if responses && session.len() == 1 {
        // xAI's first-request example is `input` as a string. Later turns
        // use the role/content array so the opening is not sent again as
        // a new conversation.
        serde_json::Value::String(session[0].content.clone())
    } else {
        serde_json::Value::Array(
            session
                .iter()
                .map(|message| {
                    serde_json::json!({
                        "role": message.role,
                        "content": message.content,
                    })
                })
                .collect(),
        )
    };
    if responses {
        serde_json::json!({
            "model": model,
            "input": input,
            "max_output_tokens": 80,
            "store": false,
            // grok-4.6 defaults to high: 16s and hundreds of think tokens
            // for a two-line Behavior pick.
            "reasoning": { "effort": "low" },
        })
    } else {
        serde_json::json!({
            "model": model,
            "messages": input,
            "max_tokens": 80,
        })
    }
}

fn content_from_body(body: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| error.to_string())?;
    if let Some(text) = value["choices"][0]["message"]["content"].as_str() {
        return Ok(text.to_string());
    }
    if let Some(text) = value["output_text"]
        .as_str()
        .filter(|text| !text.is_empty())
    {
        return Ok(text.to_string());
    }
    if let Some(items) = value["output"].as_array() {
        for item in items {
            if let Some(parts) = item["content"].as_array() {
                for part in parts {
                    if let Some(text) = part["text"].as_str().filter(|text| !text.is_empty()) {
                        return Ok(text.to_string());
                    }
                }
            }
        }
    }
    Err("model reply had no text content".to_string())
}

/// One model call in flight. The frame loop starts it and polls `try_take`.
pub struct InFlight {
    tx: Sender<Wake>,
    rx: Receiver<Wake>,
    busy: Arc<AtomicBool>,
}

impl InFlight {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn ready(&self) -> bool {
        !self.busy.load(Ordering::SeqCst)
    }

    pub fn start<C: Completer + Send + Sync + 'static>(
        &self,
        director: Arc<ModelDirector<C>>,
        context: Context,
    ) {
        self.busy.store(true, Ordering::SeqCst);
        let tx = self.tx.clone();
        thread::spawn(move || {
            // Always send. A panic here would leave `busy` set and skip
            // StaticDirector on later ticks.
            let wake =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| director.wake(&context)))
                    .unwrap_or(Wake::Failed);
            let _ = tx.send(wake);
        });
    }

    pub fn try_take(&self) -> Option<Wake> {
        match self.rx.try_recv() {
            Ok(wake) => {
                self.busy.store(false, Ordering::SeqCst);
                Some(wake)
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chat_completion_body_yields_the_message_content() {
        let body = r#"{"choices":[{"message":{"content":"stroll\nhey"}}]}"#;
        assert_eq!(content_from_body(body).unwrap(), "stroll\nhey");
    }

    #[test]
    fn a_responses_body_yields_the_output_text() {
        let body = r#"{
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "stroll\nhey"}]
            }]
        }"#;
        assert_eq!(content_from_body(body).unwrap(), "stroll\nhey");
    }

    #[test]
    fn a_body_without_content_is_an_error() {
        assert!(content_from_body("{}").is_err());
        assert!(content_from_body("not json").is_err());
    }

    #[test]
    fn a_responses_request_uses_input_and_does_not_store() {
        let session = [Message {
            role: "user",
            content: "wave".to_string(),
        }];
        let body = request_body("grok-4.6", &session, true);
        assert_eq!(body["input"], "wave");
        assert_eq!(body["max_output_tokens"], 80);
        assert_eq!(body["store"], false);
        assert_eq!(body["reasoning"]["effort"], "low");
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn a_follow_up_responses_request_sends_the_session() {
        let session = [
            Message {
                role: "user",
                content: "hello".to_string(),
            },
            Message {
                role: "assistant",
                content: "wave".to_string(),
            },
            Message {
                role: "user",
                content: "what just happened: thrown".to_string(),
            },
        ];
        let body = request_body("grok-4.6", &session, true);
        assert_eq!(body["input"][2]["content"], "what just happened: thrown");
        assert!(body["input"].is_array());
    }

    #[test]
    fn the_completions_url_joins_a_base_without_doubling_v1() {
        assert_eq!(
            completions_url("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            completions_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/chat/completions"
        );
        assert_eq!(
            completions_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            completions_url("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            completions_url("https://api.x.ai"),
            "https://api.x.ai/v1/responses"
        );
        assert_eq!(
            completions_url("https://api.x.ai/v1"),
            "https://api.x.ai/v1/responses"
        );
        assert_eq!(
            completions_url("https://api.x.ai/v1/chat/completions"),
            "https://api.x.ai/v1/chat/completions",
            "an explicit legacy path is honoured"
        );
        assert_eq!(
            completions_url("https://mtls.api.x.ai"),
            "https://mtls.api.x.ai/v1/responses"
        );
    }

    #[test]
    fn a_quoted_key_is_trimmed() {
        assert_eq!(trim_key("  sk-abc\n").as_deref(), Some("sk-abc"));
        assert_eq!(trim_key("\"sk-abc\"").as_deref(), Some("sk-abc"));
        assert_eq!(trim_key("   ").as_deref(), None);
    }

    /// Unset is a choice. `$XAI_API_KEY` expanding to nothing is a mistake
    /// that used to look the same. The log has to tell them apart.
    #[test]
    fn a_blank_provided_key_is_invalid_and_unset_is_not() {
        assert_eq!(key_from_raw(None), KeyRead::Unset);
        assert_eq!(key_from_raw(Some("")), KeyRead::Invalid);
        assert_eq!(key_from_raw(Some("  \n")), KeyRead::Invalid);
        assert_eq!(key_from_raw(Some("\"\"")), KeyRead::Invalid);
        assert!(matches!(key_from_raw(Some("sk-abc")), KeyRead::Present(_)));
    }

    #[test]
    fn startup_always_names_the_director_mode() {
        let static_only = DirectorConfig {
            enabled: false,
            configured: false,
            key_invalid: false,
            wake_every: WAKE_EVERY,
            ambient_first: Pace::FIRST,
        };
        assert_eq!(startup_lines(&static_only), ["director: StaticDirector"]);

        let blank = DirectorConfig {
            key_invalid: true,
            ..static_only.clone()
        };
        let warned = startup_lines(&blank);
        assert!(
            warned.iter().any(|line| line.contains("warning")
                && line.contains(API_KEY)
                && line.contains("StaticDirector")),
            "{warned:?}"
        );
        assert!(
            warned.iter().any(|line| line == "director: StaticDirector"),
            "{warned:?}"
        );

        let model = DirectorConfig {
            enabled: true,
            configured: true,
            key_invalid: false,
            wake_every: WAKE_EVERY,
            ambient_first: Duration::from_secs(45),
        };
        assert_eq!(
            startup_lines(&model),
            ["director: model, ambient first 45s"]
        );

        let off = DirectorConfig {
            enabled: false,
            configured: true,
            key_invalid: false,
            wake_every: WAKE_EVERY,
            ambient_first: Pace::FIRST,
        };
        assert_eq!(startup_lines(&off), ["director: off; using StaticDirector"]);
    }

    #[test]
    fn xai_has_a_legacy_alternate_and_openai_does_not() {
        assert_eq!(
            alternate_url("https://api.x.ai/v1/responses").as_deref(),
            Some("https://api.x.ai/v1/chat/completions")
        );
        assert_eq!(
            alternate_url("https://api.openai.com/v1/chat/completions"),
            None
        );
    }

    #[test]
    fn responses_falls_back_only_when_refused() {
        let url = "https://api.x.ai/v1/responses";
        assert!(fallback_url(url, "https://api.x.ai/v1/responses: status 403 {}").is_some());
        assert!(fallback_url(url, "https://api.x.ai/v1/responses: status 404").is_some());
        assert!(fallback_url(url, "https://api.x.ai/v1/responses: status 401").is_none());
        assert!(fallback_url(url, "https://api.x.ai/v1/responses: status 400").is_none());
    }

    #[test]
    fn a_status_error_keeps_the_body() {
        let error = status_error("https://api.x.ai/v1/responses", 403, " {\"error\":\"no\"} ");
        assert!(error.contains("status 403"));
        assert!(error.contains("\"error\":\"no\""));
    }
}
