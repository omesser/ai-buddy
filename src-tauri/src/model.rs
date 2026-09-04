//! Completer stand-in, env config, and the in-flight session call.
//!
//! ponytail: HTTP chat-completions until #16 attaches a Harness. The
//! `Completer` trait is the seam; this file is the disposable impl. ADR-0008.
//!
//! The Completer runs on a worker thread. The frame loop only `try_recv`s.
//! #18 binds these settings. Until then they come from the env.

use std::net::IpAddr;
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

/// `pub(crate)` so the settings window can name the variable that owns a row
/// (#272).
pub(crate) const API_KEY: &str = "AI_BUDDY_DIRECTOR_API_KEY";
pub(crate) const BASE_URL: &str = "AI_BUDDY_DIRECTOR_BASE_URL";
pub(crate) const MODEL: &str = "AI_BUDDY_DIRECTOR_MODEL";
const ENABLED: &str = "AI_BUDDY_DIRECTOR";
/// First ambient session wait, in seconds. Not a heartbeat.
const WAKE_SECS: &str = "AI_BUDDY_DIRECTOR_WAKE_SECS";

/// Completer timeout, in seconds, and the reply cap, in tokens. Both have a
/// local default that differs from the hosted one; these override either.
const TIMEOUT_SECS: &str = "AI_BUDDY_DIRECTOR_TIMEOUT_SECS";
const MAX_TOKENS: &str = "AI_BUDDY_DIRECTOR_MAX_TOKENS";

const DEFAULT_BASE: &str = "https://api.openai.com";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// A cold local server loads weights on the first call, which can outlast a
/// hosted request several times over. Losing that one wake would leave the
/// buddy quietly Static for the rest of the session.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(120);

/// Hosted replies are two lines. A local reasoning model (Qwen3, gpt-oss)
/// thinks in the same budget on chat-completions, so 80 tokens can be spent
/// before it writes anything, and the empty reply parses as garbage. Raising
/// the cap is the portable half of that fix: `reasoning_effort` is not a
/// field every one of these servers accepts, and a strict one rejects the
/// whole request over it.
const LOCAL_MAX_TOKENS: u32 = 512;
const HOSTED_MAX_TOKENS: u32 = 80;

/// Last user turn and the config that produced it. #18 displays this.
#[derive(Clone, Debug, Serialize)]
pub struct DirectorInspect {
    pub enabled: bool,
    pub configured: bool,
    pub ambient_wakes: bool,
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
    /// Proactive session wakes. Off keeps reactive wakes and Static idle life.
    pub ambient_allowed: bool,
}

impl DirectorConfig {
    pub fn inspect(&self) -> DirectorInspect {
        DirectorInspect {
            enabled: self.enabled,
            configured: self.configured,
            ambient_wakes: self.ambient_allowed,
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

/// Resolved base URL, model, and key before they become a Completer.
///
/// `api_key` empty means unset or invalid. `key_invalid` means the winning
/// source was set but unusable.
#[derive(Clone)]
pub struct DirectorSettings {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub key_invalid: bool,
}

impl std::fmt::Debug for DirectorSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectorSettings")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("key_fingerprint", &key_fingerprint(&self.api_key))
            .field("key_invalid", &self.key_invalid)
            .finish()
    }
}

/// Env first, then persisted settings, then defaults. Does not write env.
///
/// Empty env values fall through (a blank override is treated as unset).
/// For the key, env Invalid still wins over a stored key: the process asked
/// to override.
pub fn resolve(
    persisted_base: &str,
    persisted_model: &str,
    stored_key: Option<&str>,
) -> DirectorSettings {
    let base_url = resolve_string(BASE_URL, persisted_base, DEFAULT_BASE);
    let model = resolve_string(MODEL, persisted_model, DEFAULT_MODEL);
    let key = match key_from_env() {
        KeyRead::Unset => key_from_raw(stored_key),
        other => other,
    };
    let (api_key, key_invalid) = match key {
        KeyRead::Present(key) => (key, false),
        KeyRead::Invalid => (String::new(), true),
        KeyRead::Unset => (String::new(), false),
    };
    DirectorSettings {
        base_url,
        model,
        api_key,
        key_invalid,
    }
}

fn resolve_string(var: &str, persisted: &str, default: &str) -> String {
    match env_override(var) {
        Some(value) => value,
        None if !persisted.is_empty() => persisted.to_string(),
        None => default.to_string(),
    }
}

/// What `var` will impose on the file, if the process exported one.
///
/// The one place that decides env precedence, so the settings window can ask
/// the same question `resolve` answers instead of guessing at it. Empty is
/// unset: a `$VAR` that expanded to nothing is a mistake, not an override.
pub(crate) fn env_override(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|value| !value.is_empty())
}

/// Build Director on/off from already-resolved settings.
pub fn config_from(settings: &DirectorSettings) -> DirectorConfig {
    let configured = !settings.api_key.is_empty() || is_local(&settings.base_url);
    let enabled = configured && !off();
    DirectorConfig {
        enabled,
        configured,
        key_invalid: settings.key_invalid,
        wake_every: WAKE_EVERY,
        ambient_first: env_secs(WAKE_SECS).unwrap_or(Pace::FIRST),
        ambient_allowed: true,
    }
}

/// An OpenAI-compatible chat Completer from already-resolved settings, or
/// `None` when a remote host has no key set.
pub fn endpoint_from(settings: &DirectorSettings) -> Option<Endpoint> {
    let local = is_local(&settings.base_url);
    let api_key = if !settings.api_key.is_empty() {
        settings.api_key.clone()
    } else if local {
        // `headers` omits Authorization when the key is empty, so a local
        // server sees a plain request rather than a made-up Bearer token.
        String::new()
    } else {
        return None;
    };
    Some(Endpoint {
        api_key,
        url: completions_url(&settings.base_url),
        model: settings.model.clone(),
        timeout: timeout_for(local),
        max_tokens: max_tokens_for(local),
        session: Mutex::new(Vec::new()),
        agent: ureq::agent(),
    })
}

/// Length and last four. Enough to tell two keys apart, not enough to use.
pub fn key_fingerprint(key: &str) -> String {
    let n = key.len();
    let last = if n >= 4 { &key[n - 4..] } else { "****" };
    format!("len={n} last={last}")
}

/// Read Director config from the env. No API key means `StaticDirector`
/// only — unless the server is on this machine or this LAN, which needs no
/// key to talk to.
///
/// Env-only wrapper. The overlay resolves from settings and the store;
/// the probe and tests still read the env alone.
#[expect(dead_code)] // env-only wrapper; overlay call sites now use config_from
pub fn config() -> DirectorConfig {
    config_from(&resolve("", "", None))
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
pub(crate) fn trim_key(raw: &str) -> Option<String> {
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

/// Has the process already settled the key on its own?
///
/// `resolve` reaches for a stored key only when the env holds none, so a true
/// answer here means reading the secret store cannot change the outcome. On
/// macOS that read is a Keychain prompt at every launch, and one bought for an
/// answer already known is the kind a user learns to click through. Set but
/// unusable still counts: the process asked to override.
pub(crate) fn env_owns_key() -> bool {
    !matches!(key_from_env(), KeyRead::Unset)
}

fn off() -> bool {
    matches!(
        std::env::var(ENABLED).ok().as_deref(),
        Some("off" | "0" | "false")
    )
}

/// A positive number of seconds from `var`, or nothing when it is unset,
/// unparsable, or zero.
fn env_secs(var: &str) -> Option<Duration> {
    let secs: u64 = std::env::var(var).ok()?.parse().ok()?;
    (secs > 0).then_some(Duration::from_secs(secs))
}

/// Is this base URL served from this machine or this LAN?
///
/// A local host (loopback, RFC1918, unique-local IPv6, or `.local`) makes
/// `AI_BUDDY_DIRECTOR_API_KEY` optional rather than required: the user may
/// leave it unset when the server has no auth (Ollama, mlx_lm.server) or set
/// it when the server requires one (oMLX, llama.cpp with `--api-key`, vLLM
/// with `--api-key`). A remote host still requires a real key.
fn is_local(base: &str) -> bool {
    let host = base.split("://").nth(1).unwrap_or(base);
    let host = host.split('/').next().unwrap_or(host);
    // Userinfo first: in `10.0.0.1@172.16.evil.com` the digits belong to the
    // credentials, and the request goes to evil.com.
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = match host.strip_prefix('[') {
        // An IPv6 literal is bracketed, so the colons inside are not a port.
        Some(rest) => rest.split(']').next().unwrap_or(rest),
        None => host.rsplit_once(':').map_or(host, |(host, _)| host),
    };
    // A fully-qualified name ends in a dot, and DNS reads it as the same name.
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".local") {
        return true;
    }
    // Parse the whole host as an address rather than picking numbers out of
    // it: `10.0.0.5.evil.com` is a remote name that merely opens with one.
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
        // fc00::/7 is the IPv6 private range. `Ipv6Addr::is_unique_local` is
        // still unstable, and this repo builds on the pinned stable toolchain.
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.octets()[0] & 0xfe == 0xfc,
        Err(_) => false,
    }
}

fn timeout_for(local: bool) -> Duration {
    if let Some(secs) = env_secs(TIMEOUT_SECS) {
        return secs;
    }
    if local {
        LOCAL_TIMEOUT
    } else {
        TIMEOUT
    }
}

fn max_tokens_for(local: bool) -> u32 {
    // Guarded like `env_secs`: a zero cap would ask for a reply with no room
    // to answer in.
    if let Some(cap) = std::env::var(MAX_TOKENS)
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
        .filter(|&cap| cap > 0)
    {
        return cap;
    }
    if local {
        LOCAL_MAX_TOKENS
    } else {
        HOSTED_MAX_TOKENS
    }
}

/// An OpenAI-compatible chat Completer, or `None` when a remote host has no
/// key set.
///
/// Env-only wrapper. The overlay resolves from settings and the store;
/// the probe and tests still read the env alone.
pub fn endpoint() -> Option<Endpoint> {
    endpoint_from(&resolve("", "", None))
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
    max_tokens: u32,
    /// Opening + replies, so a follow-up can be short. ADR-0008.
    session: Mutex<Vec<Message>>,
    /// Held rather than built per call: `ureq::get`/`ureq::post` are "Run on a
    /// use-once [Agent]", so each wake would throw away the pooled connection
    /// and pay another TCP and TLS handshake to the model host.
    agent: ureq::Agent,
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
        key_fingerprint(&self.api_key)
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
        let request = self
            .headers(self.agent.get(url))
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(self.timeout))
            .build();
        match request.call() {
            Ok(response) => read_response(response),
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
        let body = request_body(&self.model, &snapshot, uses_responses(url), self.max_tokens);
        let request = self
            .headers(self.agent.post(url))
            .header("Content-Type", "application/json")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(self.timeout))
            .build();
        let text = match request.send_json(body) {
            Ok(response) => {
                let (code, text) = read_response(response)?;
                if (200..300).contains(&code) {
                    text
                } else {
                    self.session.lock().expect("session lock").pop();
                    return Err(status_error(url, code, &text));
                }
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

    fn headers<B>(&self, request: ureq::RequestBuilder<B>) -> ureq::RequestBuilder<B> {
        let mut request = request
            .header("User-Agent", "ai-buddy")
            .header("Accept", "application/json");
        if !self.api_key.is_empty() {
            request = request.header("Authorization", &format!("Bearer {}", self.api_key));
        }
        // Anthropic's OpenAI layer accepts Bearer; the native Messages path
        // wants these two. Sending both covers either.
        if self.url.contains("api.anthropic.com") {
            request = request.header("anthropic-version", "2023-06-01");
            if !self.api_key.is_empty() {
                request = request.header("x-api-key", &self.api_key);
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

fn read_response(mut response: ureq::http::Response<ureq::Body>) -> Result<(u16, String), String> {
    let code = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
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

/// Does a served model id name the model that was asked for?
///
/// Ollama reports `llama3.2:latest` for the `llama3.2` a user types, so an
/// exact comparison would report a served model as missing.
fn model_matches(served: &str, wanted: &str) -> bool {
    served == wanted || served.trim_end_matches(":latest") == wanted.trim_end_matches(":latest")
}

/// Read a `/v1/models` answer. Pure, so the decision is testable without a
/// server: the caller does the HTTP and the naming.
///
/// A body this cannot read gets the benefit of the doubt: MLX and some
/// llama.cpp builds answer without a `data` list, and calling their model
/// absent would be worse than saying nothing. An empty `data` is different —
/// that is a server saying plainly it serves nothing, which is worth hearing.
fn preflight_verdict(models: Result<(u16, String), String>, model: &str) -> Result<(), String> {
    let (code, body) = models.map_err(|error| format!("unreachable: {error}"))?;
    if !(200..300).contains(&code) {
        return Err(format!("/v1/models answered {code}"));
    }
    let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
    let Some(items) = parsed.as_ref().and_then(|value| value["data"].as_array()) else {
        return Ok(());
    };
    let served: Vec<&str> = items
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    if served.is_empty() {
        return Err("is up and serving no models".to_string());
    }
    if served.iter().any(|id| model_matches(id, model)) {
        return Ok(());
    }
    Err(format!(
        "model {model:?} is not served; it has {}",
        served.join(", ")
    ))
}

/// Say once, in the background, whether the configured server is actually
/// there. Diagnostic only: a wake that fails already falls to
/// `StaticDirector` per turn, so this changes no behaviour — it exists
/// because "the buddy went quiet" is otherwise unexplained.
///
/// Spawned rather than awaited. ADR-0004 keeps the model off the frame loop,
/// and a stopped server would otherwise hold up startup for the timeout.
pub fn spawn_preflight(settings: &DirectorSettings) {
    let Some(endpoint) = endpoint_from(settings) else {
        return;
    };
    thread::spawn(move || {
        let origin = endpoint.origin();
        let models = endpoint.get(&format!("{origin}/v1/models"));
        match preflight_verdict(models, endpoint.model()) {
            Ok(()) => {
                if tracing() {
                    eprintln!("director: {origin} answered, model {}", endpoint.model());
                }
            }
            // A transport error already quotes the URL it failed to reach;
            // naming the origin again would say it twice.
            Err(why) => {
                let reason = if why.contains(&origin) {
                    why
                } else {
                    format!("{origin} {why}")
                };
                eprintln!("director: {reason}; staying on StaticDirector until it answers");
            }
        }
    });
}

const PING: &str = "Reply with the single word pong and nothing else.";

/// Same Completer the overlay uses, without starting the overlay.
///
/// `scripts/probe-model.sh` is the face of this. Later a Harness attach
/// (#16) can share the command: same env, same exit codes, a second hop.
pub fn run_probe() -> i32 {
    let Some(endpoint) = endpoint() else {
        eprintln!(
            "probe-model: no AI_BUDDY_DIRECTOR_API_KEY, and \
             AI_BUDDY_DIRECTOR_BASE_URL is not a local server"
        );
        return 2;
    };

    println!("probe-model");
    println!("  url    {}", endpoint.url());
    println!("  model  {}", endpoint.model());
    if endpoint.api_key.is_empty() {
        println!("  key    none (local server)");
    } else {
        println!("  key    {}", endpoint.key_fingerprint());
    }
    println!();

    let origin = endpoint.origin();
    let models = endpoint.get(&format!("{origin}/v1/models"));
    probe_result(&format!("{origin}/v1/models"), &models);
    match preflight_verdict(models, endpoint.model()) {
        Ok(()) => println!("  model {} is served", endpoint.model()),
        Err(why) => println!("  {why}"),
    }
    println!();
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
    let answer = endpoint.get(url);
    probe_result(url, &answer);
    println!();
}

fn probe_result(url: &str, answer: &Result<(u16, String), String>) {
    println!("GET {url}");
    match answer {
        Ok((code, body)) => println!("  {code} {}", clip_body(body)),
        Err(error) => println!("  transport {error}"),
    }
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

fn request_body(
    model: &str,
    session: &[Message],
    responses: bool,
    max_tokens: u32,
) -> serde_json::Value {
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
            "max_output_tokens": max_tokens,
            "store": false,
            // grok-4.6 defaults to high: 16s and hundreds of think tokens
            // for a two-line Behavior pick.
            "reasoning": { "effort": "low" },
        })
    } else {
        serde_json::json!({
            "model": model,
            "messages": input,
            "max_tokens": max_tokens,
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

    /// Drop a call that no longer belongs to this Character.
    ///
    /// The worker still finishes; its Wake lands on a channel nobody reads.
    /// ureq cannot abort a POST already on the wire.
    pub fn cancel(&mut self) {
        *self = Self::new();
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

/// Drop an in-flight wake and install a Completer for the new settings.
///
/// The worker still finishes; its Wake lands on a channel nobody reads.
/// ureq cannot abort a POST already on the wire.
pub fn retarget_model(
    pending: &mut InFlight,
    in_flight: &mut Option<Context>,
    model: &mut Option<Arc<ModelDirector<Endpoint>>>,
    behaviors: impl IntoIterator<Item = impl Into<String>>,
    settings: &DirectorSettings,
    configured: bool,
) {
    pending.cancel();
    *in_flight = None;
    *model = configured.then(|| {
        Arc::new(ModelDirector::new(
            endpoint_from(settings).expect("configured means a Completer exists"),
            behaviors,
        ))
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Run `body` with the three Director vars set as given, then restored.
    ///
    /// One lock for the whole test binary: `settings` and `settings::form`
    /// test env-owned rows against the same vars, and a second mutex would
    /// not serialise against this one.
    pub(crate) fn with_env(
        key: Option<&str>,
        base: Option<&str>,
        model: Option<&str>,
        body: impl FnOnce(),
    ) {
        // Concurrent setenv/getenv is undefined behaviour. These three vars are
        // process-global and the resolve tests share them; serialise mutation.
        static ENV: Mutex<()> = Mutex::new(());
        let _lock = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        struct Guard {
            key: Option<String>,
            base: Option<String>,
            model: Option<String>,
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                restore(API_KEY, self.key.take());
                restore(BASE_URL, self.base.take());
                restore(MODEL, self.model.take());
            }
        }

        fn save(var: &str) -> Option<String> {
            std::env::var(var).ok()
        }

        fn restore(var: &str, previous: Option<String>) {
            match previous {
                Some(value) => std::env::set_var(var, value),
                None => std::env::remove_var(var),
            }
        }

        fn apply(var: &str, value: Option<&str>) {
            match value {
                Some(value) => std::env::set_var(var, value),
                None => std::env::remove_var(var),
            }
        }

        let _guard = Guard {
            key: save(API_KEY),
            base: save(BASE_URL),
            model: save(MODEL),
        };
        apply(API_KEY, key);
        apply(BASE_URL, base);
        apply(MODEL, model);
        body();
    }

    #[test]
    fn env_beats_persisted_base_and_model() {
        with_env(None, Some("https://api.x.ai"), Some("grok-4.6"), || {
            let settings = resolve("https://api.openai.com", "gpt-4o-mini", Some("sk-stored"));
            assert_eq!(settings.base_url, "https://api.x.ai");
            assert_eq!(settings.model, "grok-4.6");
        });
    }

    #[test]
    fn persisted_is_used_when_env_is_unset() {
        with_env(None, None, None, || {
            let settings = resolve("https://api.x.ai", "grok-4.6", Some("sk-stored-key"));
            assert_eq!(settings.base_url, "https://api.x.ai");
            assert_eq!(settings.model, "grok-4.6");
            assert_eq!(settings.api_key, "sk-stored-key");
            assert!(!settings.key_invalid);
        });
    }

    #[test]
    fn env_key_beats_the_stored_key() {
        with_env(Some("sk-env-key"), None, None, || {
            let settings = resolve("", "", Some("sk-stored-key"));
            assert_eq!(settings.api_key, "sk-env-key");
        });
    }

    #[test]
    fn invalid_env_beats_store() {
        with_env(Some(""), None, None, || {
            let settings = resolve(
                "https://api.openai.com",
                "gpt-4o-mini",
                Some("sk-stored-key"),
            );
            assert!(
                settings.api_key.is_empty(),
                "a blank env key must not fall through to the store"
            );
            assert!(settings.key_invalid);
        });
    }

    #[test]
    fn a_remote_url_without_a_key_is_not_configured() {
        with_env(None, None, None, || {
            let settings = resolve("https://api.openai.com", "gpt-4o-mini", None);
            let config = config_from(&settings);
            assert!(!config.configured);
            assert!(endpoint_from(&settings).is_none());
        });
    }

    #[test]
    fn a_local_url_without_a_key_is_configured() {
        with_env(None, None, None, || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let config = config_from(&settings);
            assert!(config.configured);
            let endpoint = endpoint_from(&settings).expect("local needs no key");
            assert!(endpoint.url().contains("11434"));
            assert_eq!(endpoint.model(), "gemma4");
        });
    }

    #[test]
    fn resolve_does_not_write_env() {
        with_env(None, None, None, || {
            let _ = resolve("https://api.x.ai", "grok-4.6", Some("sk-stored"));
            assert!(std::env::var("AI_BUDDY_DIRECTOR_API_KEY").is_err());
            assert!(std::env::var("AI_BUDDY_DIRECTOR_BASE_URL").is_err());
        });
    }

    #[test]
    fn director_settings_debug_prints_the_fingerprint_not_the_key() {
        with_env(None, None, None, || {
            let settings = resolve("", "", Some("sk-super-secret-key"));
            let dump = format!("{settings:?}");
            assert!(
                !dump.contains("sk-super-secret-key"),
                "Debug must not echo the key: {dump}"
            );
            assert!(
                dump.contains("key_fingerprint"),
                "Debug should name the fingerprint field: {dump}"
            );
            assert!(
                dump.contains(&key_fingerprint("sk-super-secret-key")),
                "Debug should name the fingerprint: {dump}"
            );
        });
    }

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
        let body = request_body("grok-4.6", &session, true, HOSTED_MAX_TOKENS);
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
        let body = request_body("grok-4.6", &session, true, HOSTED_MAX_TOKENS);
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
            ambient_allowed: true,
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
            ambient_allowed: true,
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
            ambient_allowed: true,
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
    fn a_loopback_or_private_base_is_served_from_here() {
        assert!(is_local("http://localhost:11434"), "Ollama");
        assert!(is_local("http://127.0.0.1:8080"), "llama.cpp");
        assert!(is_local("http://[::1]:1234"), "LM Studio over IPv6");
        assert!(is_local("http://192.168.1.50:8000"), "a box on the LAN");
        assert!(is_local("http://10.0.0.5:8000"));
        assert!(is_local("http://172.16.4.2:8080"));
        assert!(is_local("http://studio.local:1234"), "mDNS");
        assert!(is_local("http://[fd00::1]:8080"), "an IPv6 private address");
        assert!(!is_local("https://api.openai.com"));
        assert!(!is_local("https://api.x.ai"));
        assert!(
            !is_local("http://172.32.0.1:8080"),
            "just outside the private range"
        );
        assert!(
            !is_local("https://localhost.example.com"),
            "a hostname that merely starts with localhost"
        );
    }

    #[test]
    fn a_remote_host_wearing_an_address_is_still_remote() {
        // Picking the numbers out of a name would read every one of these as
        // a machine on this LAN, and hand it a keyless Character Prompt.
        assert!(!is_local("http://10.0.0.5.evil.com:8080"));
        assert!(!is_local("http://192.168.1.1.attacker.net"));
        assert!(
            !is_local("http://api.10.0.0.5.example.com"),
            "digits in the middle of the name"
        );
        assert!(
            !is_local("http://10.0.0.1@172.16.evil.com/"),
            "the digits are userinfo; the host is evil.com"
        );
        assert!(
            is_local("http://user@10.0.0.1"),
            "userinfo before a real one"
        );
    }

    #[test]
    fn a_host_is_matched_however_it_is_spelled() {
        assert!(is_local("http://LOCALHOST:11434"));
        assert!(is_local("http://Localhost"));
        assert!(is_local("http://STUDIO.LOCAL:1234"));
        assert!(is_local("http://localhost.:11434"), "fully qualified");
    }

    #[test]
    fn a_cold_local_model_gets_room_a_hosted_one_does_not_need() {
        assert!(timeout_for(true) > timeout_for(false));
        assert!(max_tokens_for(true) > max_tokens_for(false));
    }

    #[test]
    fn the_preflight_passes_when_the_server_lists_the_model() {
        let body = r#"{"data":[{"id":"llama3.2:latest"},{"id":"qwen3:8b"}]}"#;
        let ok = Ok((200, body.to_string()));
        assert!(preflight_verdict(ok.clone(), "qwen3:8b").is_ok());
        assert!(
            preflight_verdict(ok, "llama3.2").is_ok(),
            "Ollama reports a :latest tag the user does not type"
        );
    }

    #[test]
    fn the_preflight_names_why_it_did_not_pass() {
        let down = preflight_verdict(Err("connection refused".to_string()), "llama3.2");
        assert!(down.unwrap_err().contains("connection refused"));

        let refused = preflight_verdict(Ok((404, String::new())), "llama3.2");
        assert!(refused.unwrap_err().contains("404"));

        let missing = preflight_verdict(
            Ok((200, r#"{"data":[{"id":"qwen3:8b"}]}"#.to_string())),
            "llama3.2",
        );
        let missing = missing.unwrap_err();
        assert!(missing.contains("llama3.2"), "{missing}");
        assert!(missing.contains("qwen3:8b"), "names what is served");
    }

    #[test]
    fn a_body_this_cannot_read_is_left_alone() {
        // MLX and some llama.cpp builds answer without a `data` list. A probe
        // that cannot see the model must not claim it is absent.
        assert!(preflight_verdict(Ok((200, "not json".to_string())), "any").is_ok());
        assert!(preflight_verdict(Ok((200, r#"{"models":["a"]}"#.to_string())), "any").is_ok());
    }

    #[test]
    fn a_server_serving_nothing_says_so() {
        // Ollama with nothing pulled answers 200 with an empty list. That is
        // knowable, and the reason the buddy is about to stay quiet.
        let empty = preflight_verdict(Ok((200, r#"{"data":[]}"#.to_string())), "gemma4");
        assert!(empty.unwrap_err().contains("serving no models"));
    }

    #[test]
    fn a_status_error_keeps_the_body() {
        let error = status_error("https://api.x.ai/v1/responses", 403, " {\"error\":\"no\"} ");
        assert!(error.contains("status 403"));
        assert!(error.contains("\"error\":\"no\""));
    }

    #[test]
    fn a_present_key_is_used_even_when_the_base_is_local() {
        with_env(None, None, None, || {
            let settings = resolve(
                "http://localhost:8000",
                "local-model",
                Some("omlx-test-key"),
            );
            assert!(
                is_local(&settings.base_url),
                "precondition: the base is local"
            );
            let endpoint = endpoint_from(&settings).expect("local is configured");
            assert_eq!(
                endpoint.key_fingerprint(),
                key_fingerprint("omlx-test-key"),
                "a present key must not be dropped for a local base"
            );
        });
    }

    struct Slow;

    impl Completer for Slow {
        fn complete(&self, _: &str) -> Result<String, String> {
            thread::sleep(Duration::from_millis(40));
            Ok("idle".to_string())
        }
    }

    /// A Context to stand in for a wake already on the wire. `pub(crate)` for
    /// the `settings` tests, which retarget through the same call.
    pub(crate) fn wake_context() -> Context {
        use ai_buddy_core::engine::State;
        use ai_buddy_core::sensing::Activity;
        use std::time::UNIX_EPOCH;

        Context {
            activity: Activity {
                frontmost_application: None,
                switched: false,
                idle: Duration::ZERO,
                at: UNIX_EPOCH,
                hour: 12,
                minute: 0,
                displays_asleep: false,
            },
            recent: Vec::new(),
            personality: String::new(),
            state: State::Grounded,
            happened: ai_buddy_core::director::Happened::Ambient,
            standing: String::new(),
        }
    }

    #[test]
    fn retarget_drops_an_in_flight_wake_and_installs_the_new_completer() {
        with_env(None, None, None, || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let config = config_from(&settings);
            let mut pending = InFlight::new();
            let mut in_flight = Some(wake_context());
            let mut model = None;
            retarget_model(
                &mut pending,
                &mut in_flight,
                &mut model,
                ["stroll"],
                &settings,
                config.configured,
            );
            assert!(pending.ready(), "cancel replaced the channel");
            assert!(in_flight.is_none());
            assert!(model.is_some());
        });
    }

    #[test]
    fn retarget_to_a_remote_without_a_key_leaves_static() {
        with_env(None, None, None, || {
            let settings = resolve("https://api.openai.com", "gpt-4o-mini", None);
            let config = config_from(&settings);
            let mut pending = InFlight::new();
            let mut in_flight = None;
            let mut model = None;
            retarget_model(
                &mut pending,
                &mut in_flight,
                &mut model,
                ["stroll"],
                &settings,
                config.configured,
            );
            assert!(model.is_none());
        });
    }

    #[test]
    fn retarget_installs_when_configured_even_if_director_is_off() {
        with_env(None, None, None, || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let mut config = config_from(&settings);
            config.enabled = false;
            assert!(config.configured, "local needs no key");
            let mut pending = InFlight::new();
            let mut in_flight = None;
            let mut model = None;
            retarget_model(
                &mut pending,
                &mut in_flight,
                &mut model,
                ["stroll"],
                &settings,
                config.configured,
            );
            assert!(
                model.is_some(),
                "Director off must still leave a Completer for ToggleDirector"
            );
        });
    }

    /// A switch must not apply the old Character's reply, and must be able
    /// to start the new opening before that POST returns.
    #[test]
    fn cancel_drops_a_wake_that_still_arrives() {
        let pending = InFlight::new();
        let director = Arc::new(ModelDirector::new(Slow, ["idle"]));
        pending.start(director, wake_context());
        assert!(!pending.ready(), "the call is in flight");

        let mut pending = pending;
        pending.cancel();
        assert!(
            pending.ready(),
            "a cancelled call must not block the next Character Prompt"
        );
        thread::sleep(Duration::from_millis(80));
        assert!(
            pending.try_take().is_none(),
            "the abandoned Wake must not land on the new Character"
        );
    }

    /// #175: how often a live local model breaks the reply contract, as the
    /// before number #144 argues from. Ignored because it needs a server and
    /// spends real seconds; it is the harness, not a check of our own code.
    ///
    /// The classifier is `ModelDirector::wake` itself rather than a copy of
    /// it, so the measurement cannot drift from what the app actually does:
    /// a proposal naming a declared Behavior is accepted, an empty name is
    /// `as_speech` catching prose, and `Failed` is the turn `StaticDirector`
    /// takes. One session throughout, because that is how the buddy runs.
    ///
    /// ```sh
    /// AI_BUDDY_DIRECTOR_BASE_URL=http://localhost:11434 \
    /// AI_BUDDY_DIRECTOR_MODEL=gemma4 \
    /// cargo test -p ai-buddy measure_the_reply_contract -- --ignored --nocapture
    /// ```
    ///
    /// `AI_BUDDY_BENCH_WAKES` sets the sample size; it defaults to 40.
    #[test]
    #[ignore]
    fn measure_the_reply_contract_failure_rate() {
        use ai_buddy_core::director::{Context, Happened, ModelDirector, Wake};
        use ai_buddy_core::engine::State;
        use ai_buddy_core::sensing::Activity;
        use std::path::Path;
        use std::time::{Instant, SystemTime};

        // Forty tells 5% from 50%, which is what the question needs. It does
        // not tell 5% from 8%: nothing pins `temperature` or a seed, because
        // the app sends neither and this measures the app, so runs of the
        // same model wander by a few points. Raise it when a tighter number
        // is worth the minutes.
        let wakes: usize = std::env::var("AI_BUDDY_BENCH_WAKES")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .filter(|&n: &usize| n > 0)
            .unwrap_or(40);

        let endpoint = endpoint().expect("AI_BUDDY_DIRECTOR_BASE_URL and _MODEL in the env");
        let model = endpoint.model().to_string();
        let origin = endpoint.origin();

        // A real shipped package, so the prompt is the one production sends:
        // its Personality Prompt and its declared Behavior names.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../characters/cat");
        let files = crate::package::read(&root).expect("the shipped cat package reads");
        let cat = ai_buddy_core::character::load(&files).expect("and loads");
        let behaviors: Vec<String> = cat.behaviors.keys().cloned().collect();

        let director = ModelDirector::new(endpoint, behaviors.clone());

        // Vary the wake so the prompts differ: the reactive verbs plus ambient.
        let occasions = [
            (Happened::Poke, State::Grounded, "the display floor"),
            (Happened::Throw, State::Falling, "nothing"),
            (Happened::Summon, State::Grounded, "a Terminal window"),
            (Happened::Perch, State::Perched, "a Safari window"),
            (Happened::Ambient, State::Grounded, "the top of the Dock"),
        ];

        let (mut accepted, mut speech, mut failed) = (0usize, 0usize, 0usize);
        // A reply whose first line names a declared Behavior in the wrong
        // case is the contract kept and our matcher refusing it: `knows`
        // compares exactly. Counting it apart separates what the model got
        // wrong from what we do.
        let mut case_only = 0usize;
        let mut examples: Vec<String> = Vec::new();
        let started = Instant::now();

        for turn in 0..wakes {
            let (happened, state, standing) = &occasions[turn % occasions.len()];
            let context = Context {
                activity: Activity {
                    frontmost_application: Some("Terminal".to_string()),
                    switched: turn % 3 == 0,
                    idle: Duration::from_secs((turn as u64 % 7) * 30),
                    at: SystemTime::now(),
                    hour: 9 + (turn as u8 % 12),
                    minute: ((turn as u32 * 7) % 60) as u8,
                    displays_asleep: false,
                },
                recent: Vec::new(),
                personality: cat.personality.clone(),
                state: *state,
                happened: *happened,
                standing: standing.to_string(),
            };

            match director.wake(&context) {
                Wake::Proposed(proposal) if !proposal.behavior.is_empty() => {
                    accepted += 1;
                }
                Wake::Proposed(proposal) => {
                    // `as_speech` hands back the whole reply, so its first
                    // line is the name the model actually offered.
                    let said = proposal.dialogue.unwrap_or_default();
                    let offered = said
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .trim_end_matches(['.', ':', '!'])
                        .to_string();
                    let near = behaviors
                        .iter()
                        .any(|declared| declared.eq_ignore_ascii_case(&offered));
                    if near {
                        case_only += 1;
                    } else {
                        speech += 1;
                    }
                    if examples.len() < 6 {
                        let tag = if near { "case-only" } else { "speech" };
                        examples.push(format!("  {tag}: {}", said.replace('\n', " | ")));
                    }
                }
                Wake::Failed => {
                    failed += 1;
                    if examples.len() < 5 {
                        examples.push("  failed: unparsable or transport error".to_string());
                    }
                }
            }
        }

        let percent = |n: usize| (n as f64) * 100.0 / (wakes as f64);
        println!("\n#175 reply-contract outcomes over {wakes} wakes");
        println!("  model:     {model} at {origin}");
        println!("  behaviors: {}", behaviors.join(", "));
        println!("  elapsed:   {:.0}s", started.elapsed().as_secs_f64());
        println!("  accepted:   {accepted:>3}  ({:.0}%)", percent(accepted));
        println!(
            "  case-only:  {case_only:>3}  ({:.0}%)  contract kept, matcher refused",
            percent(case_only)
        );
        println!(
            "  speech:     {speech:>3}  ({:.0}%)  genuine prose",
            percent(speech)
        );
        println!("  failed:     {failed:>3}  ({:.0}%)", percent(failed));
        println!(
            "  the model broke the contract on {:.0}% of wakes",
            percent(speech + failed)
        );
        println!("  (sampling is the server's own; runs of one model wander a few points)");
        for line in &examples {
            println!("{line}");
        }

        assert_eq!(
            accepted + case_only + speech + failed,
            wakes,
            "every wake lands in exactly one bucket"
        );
    }
}
