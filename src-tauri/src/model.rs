//! Completer stand-in, env config, and the in-flight session call.
//!
//! ponytail: HTTP chat-completions until #16 attaches a Harness. The
//! `Completer` trait is the seam; this file is the disposable impl. ADR-0008.
//!
//! The Completer runs on a worker thread. The frame loop only polls `Slots`.
//! #18 binds these settings. Until then they come from the env.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ai_buddy_core::director::{
    Completer, Context, Happened, ModelDirector, Pace, Wake, WAKE_EVERY,
};
use ai_buddy_core::roster::InstanceId;
use serde::Serialize;

/// Completer timeout. After this, fall back to `StaticDirector`.
///
/// Longer than a snappy chat-completions hop: xAI's Responses path can
/// think, and 8s was enough to lose a Grok wake to Static.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// Prompt, raw reply, and parse. Off unless asked: a Character Prompt is
/// a paragraph, and printing it sixty times a minute would bury everything
/// else. Same gate as the hit-test and frame traces.
pub fn tracing() -> bool {
    crate::dev_flags::TRACE_DIRECTOR.is_on()
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
pub(crate) const ENABLED: &str = "AI_BUDDY_DIRECTOR";
/// First ambient session wait, in seconds. Not a heartbeat.
const WAKE_SECS: &str = "AI_BUDDY_DIRECTOR_WAKE_SECS";

/// Completer timeout, in seconds, and the reply cap, in tokens. Both have a
/// local default that differs from the hosted one; these override either.
///
/// `pub(crate)` for the same reason as the three above: the settings window
/// names the variable that owns a frozen row.
pub(crate) const TIMEOUT_SECS: &str = "AI_BUDDY_DIRECTOR_TIMEOUT_SECS";
pub(crate) const MAX_TOKENS: &str = "AI_BUDDY_DIRECTOR_MAX_TOKENS";

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
    /// What `AI_BUDDY_DIRECTOR` says, if it says anything. Read here rather
    /// than in `apply_switch`, which the frame loop calls every tick, and
    /// nothing sets the variable once the process is running.
    env_says: Option<bool>,
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
    /// Fold the saved switch in: the switch in force, and a key or a local
    /// host to make a Completer exist.
    ///
    /// The only place that composes the two into what the Director does. A
    /// caller that sets `enabled` from `configured` alone loses the variable.
    pub fn apply_switch(&mut self, saved_on: bool) {
        self.enabled = self.env_says.unwrap_or(saved_on) && self.configured;
    }

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

/// The value in force for `var`: the exported one, else the file's.
///
/// `resolve_string` without a default, for the rows whose blank means "the
/// default, whichever the endpoint turns out to be".
pub(crate) fn env_or_file(var: &str, file: &str) -> String {
    env_override(var).unwrap_or_else(|| file.to_string())
}

/// Build Director on/off from already-resolved settings.
pub fn config_from(settings: &DirectorSettings) -> DirectorConfig {
    let configured = !settings.api_key.is_empty() || is_local(&settings.base_url);
    let env_says = env_switch(ENABLED);
    DirectorConfig {
        enabled: env_says.unwrap_or(true) && configured,
        configured,
        env_says,
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
        session: Mutex::new(Session::default()),
        streams: AtomicBool::new(true),
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

/// The vocabulary every switch answers to, and the only place it is stated.
///
/// One vocabulary because two meant `=true` turning one switch on and another
/// off: the Director read its own three words and ignored the rest, while a
/// Development flag took any value at all and called everything but `1` off.
///
/// A word outside it is a typo rather than a choice, so it owns nothing and
/// whoever held the switch keeps it. `env_switch_warnings` names it at launch,
/// because a value quietly ignored looks exactly like one obeyed.
fn switch_from(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" => Some(true),
        "0" | "off" | "false" | "no" => Some(false),
        _ => None,
    }
}

/// What `var` says a switch should be, if it says anything a switch can hear.
pub(crate) fn env_switch(var: &str) -> Option<bool> {
    switch_from(&env_override(var)?)
}

/// One line per variable in `vars` holding a value no switch could read.
pub fn env_switch_warnings(vars: &[&str]) -> Vec<String> {
    vars.iter()
        .filter_map(|var| {
            let value = env_override(var)?;
            switch_from(&value).is_none().then(|| {
                format!(
                    "env: warning: {var}={value} is not on or off (1/0, true/false, yes/no); ignoring it"
                )
            })
        })
        .collect()
}

/// The Director switch in force: the exported value, else the saved one.
///
/// What the window draws and the tray checks. Deliberately not folded with
/// `configured` — the box has always shown the switch rather than whether a
/// Completer answers, and a key lives in the secret store that this layer
/// does not read (#291).
pub(crate) fn director_in_force(saved_on: bool) -> bool {
    env_switch(ENABLED).unwrap_or(saved_on)
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
    // `dev_flags` holds the value the variable or the file settled on, so the
    // precedence is not re-decided here (#273).
    if let Some(secs) = crate::dev_flags::director_timeout_secs() {
        return Duration::from_secs(secs);
    }
    if local {
        LOCAL_TIMEOUT
    } else {
        TIMEOUT
    }
}

fn max_tokens_for(local: bool) -> u32 {
    // As with the timeout, decided in `dev_flags::seed`. A zero cap is unset
    // there: a reply with no room to answer in is not a value to keep.
    if let Some(cap) = crate::dev_flags::director_max_tokens() {
        return cap;
    }
    if local {
        LOCAL_MAX_TOKENS
    } else {
        HOSTED_MAX_TOKENS
    }
}

/// What an empty Completer-timeout field means, in seconds.
///
/// Both defaults, because `describe` builds the form without settings and so
/// cannot know whether the endpoint is local. Naming one of them would make
/// the placeholder wrong for half the users.
pub(crate) fn timeout_placeholder() -> String {
    format!(
        "{} ({} for a local server)",
        TIMEOUT.as_secs(),
        LOCAL_TIMEOUT.as_secs()
    )
}

/// What an empty reply-cap field means, in tokens. See `timeout_placeholder`.
pub(crate) fn max_tokens_placeholder() -> String {
    format!("{HOSTED_MAX_TOKENS} ({LOCAL_MAX_TOKENS} for a local server)")
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

/// The conversation, and which turn is open in it.
///
/// A counter rather than the position of the last message: two calls can be
/// inside `post` at once now that a world event may supersede a wake (#312),
/// nothing orders them, and so "the question at the end" does not say whose.
#[derive(Default)]
struct Session {
    messages: Vec<Message>,
    opened: u64,
}

pub struct Endpoint {
    api_key: String,
    url: String,
    model: String,
    timeout: Duration,
    max_tokens: u32,
    /// Opening + replies, so a follow-up can be short. ADR-0008.
    session: Mutex<Session>,
    /// Does this host stream? Starts optimistic and only ever falls, once a
    /// whole reply has succeeded where a stream did not (#302).
    ///
    /// Per host, though `post` takes a `url`: a host that streamed on one of
    /// the two paths and not the other would lose streaming on both. No such
    /// host is known, and the cost if one exists is latency, not a failure.
    streams: AtomicBool,
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
            .headers(self.agent.get(url), "application/json")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(self.timeout))
            .build();
        match request.call() {
            Ok(response) => read_response(response),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Send `prompt` as the next session turn and read the reply.
    ///
    /// Takes `url` rather than using `self.url` so the probe can show a 403
    /// on `/v1/responses` next to a 200 on chat-completions, and so
    /// `complete` can retry the other xAI path.
    ///
    /// Streams, and falls back to a whole reply for a server that will not.
    /// The fallback retries the same session snapshot, so both attempts ask
    /// the same question and only one answer is ever recorded — and once a
    /// whole reply has succeeded where a stream did not, this endpoint stops
    /// asking, rather than paying two POSTs on every wake for the rest of
    /// the session.
    pub fn post(&self, url: &str, prompt: &str) -> Result<String, String> {
        let (turn, snapshot) = self.open_turn(prompt);
        let wire = if self.streams.load(Ordering::SeqCst) {
            Wire::Stream
        } else {
            Wire::Whole
        };
        let mut reply = self.send(url, &snapshot, wire);
        if let Err(unsent) = &reply {
            if let Some(settles) = unsent.retry_settles() {
                if tracing() {
                    eprintln!("director: {}; retrying without stream", unsent.why());
                }
                // A call dropped between the two attempts must not become a
                // fresh request the frame loop can no longer reach.
                reply = if abandoned() {
                    Err(Unsent::Abandoned)
                } else {
                    let whole = self.send(url, &snapshot, Wire::Whole);
                    // Evidence, not a guess: the server rejected the field
                    // and a whole reply worked, so this host does not stream.
                    // A refusal misread from some unrelated 400 fails twice
                    // and settles nothing, and neither does a stream that
                    // merely broke.
                    if whole.is_ok() && settles {
                        self.streams.store(false, Ordering::SeqCst);
                    }
                    whole
                };
            }
        }
        self.close_turn(turn, reply.map_err(Unsent::into_error))
    }

    /// Paired with `close_turn`: the session only ever grows here and is only
    /// ever trimmed there. Hands back the turn it opened, and a snapshot rather
    /// than the lock, so the fallback retry asks the identical question.
    ///
    /// A trailing question is one a superseded call left open (#312): its
    /// worker is still on the wire and no longer owns a turn here, so
    /// withdrawing it is what keeps the Completer from being asked two things
    /// at once.
    fn open_turn(&self, prompt: &str) -> (u64, Vec<Message>) {
        let mut session = self.session.lock().expect("session lock");
        if session
            .messages
            .last()
            .is_some_and(|last| last.role == "user")
        {
            session.messages.pop();
        }
        session.messages.push(Message {
            role: "user",
            content: prompt.to_string(),
        });
        session.opened += 1;
        (session.opened, session.messages.clone())
    }

    /// Record the reply, or take the question back out.
    ///
    /// A turn that produced nothing pops the user message, because the session
    /// is what the *next* prompt is built from: leaving the question behind
    /// would ask the Completer to answer two things at once.
    ///
    /// A turn some later `open_turn` has replaced touches nothing at all. Its
    /// question is already gone and the one at the end belongs to the wake that
    /// superseded it, so popping would take the winner's question out and
    /// pushing would answer it with the loser's reply — a reply `Slots::take`
    /// will never hand out anyway.
    fn close_turn(&self, turn: u64, reply: Result<String, String>) -> Result<String, String> {
        let mut session = self.session.lock().expect("session lock");
        if session.opened != turn {
            return reply;
        }
        match reply {
            Ok(content) => {
                session.messages.push(Message {
                    role: "assistant",
                    content: content.clone(),
                });
                Ok(content)
            }
            Err(error) => {
                session.messages.pop();
                Err(error)
            }
        }
    }

    /// One POST. Both attempts come through here, so the fallback differs
    /// from the first try in exactly one field.
    fn send(&self, url: &str, session: &[Message], wire: Wire) -> Result<String, Unsent> {
        let accept = match wire {
            Wire::Stream => "text/event-stream",
            Wire::Whole => "application/json",
        };
        let body = request_body(
            &self.model,
            session,
            uses_responses(url),
            self.max_tokens,
            wire,
        );
        let request = self
            .headers(self.agent.post(url), accept)
            .header("Content-Type", "application/json")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(self.timeout))
            .build();
        let response = request
            .send_json(body)
            .map_err(|error| Unsent::Failed(error.to_string()))?;

        let code = response.status().as_u16();
        if !(200..300).contains(&code) {
            let (_, text) = read_response(response).map_err(Unsent::Failed)?;
            let error = status_error(url, code, &text);
            return Err(if wire == Wire::Stream && refused_stream(code, &text) {
                Unsent::NotStreamable(error)
            } else {
                Unsent::Failed(error)
            });
        }

        match wire {
            Wire::Whole => {
                let (_, text) = read_response(response).map_err(Unsent::Failed)?;
                content_from_body(&text).map_err(|error| Unsent::Failed(format!("{url}: {error}")))
            }
            Wire::Stream => {
                // Capped like the whole-body read. `into_reader` is unlimited
                // by default, and a server that never stops sending would
                // otherwise grow this String until the machine gave out.
                let reader = response
                    .into_body()
                    .into_with_config()
                    .limit(STREAM_LIMIT)
                    .reader();
                match read_stream(reader, abandoned) {
                    Ok(Streamed::Complete(content)) if !content.trim().is_empty() => Ok(content),
                    Ok(Streamed::Complete(_)) => Err(Unsent::Failed(format!(
                        "{url}: streamed reply had no text content"
                    ))),
                    Ok(Streamed::Cut) => {
                        Err(Unsent::Cut(format!("{url}: the stream ended mid-reply")))
                    }
                    Ok(Streamed::NotEventStream) => Err(Unsent::NotStreamable(format!(
                        "{url}: answered 200 with no event stream in it"
                    ))),
                    Ok(Streamed::Abandoned) => Err(Unsent::Abandoned),
                    Err(error) => Err(Unsent::Failed(format!("{url}: {error}"))),
                }
            }
        }
    }

    fn headers<B>(
        &self,
        request: ureq::RequestBuilder<B>,
        accept: &str,
    ) -> ureq::RequestBuilder<B> {
        let mut request = request
            .header("User-Agent", "ai-buddy")
            .header("Accept", accept);
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

/// Did the server reject the request *for* asking to stream?
///
/// 400 and 422 are the codes that mean "your body is wrong", and a strict
/// OpenAI-compatible server names the field it did not recognise. Nothing
/// else counts: a 401 or 403 would fail the same way without the field, and
/// on the xAI paths 403 already means something `fallback_url` handles. The
/// cost of reading this too narrowly is one turn of `StaticDirector`.
fn refused_stream(code: u16, body: &str) -> bool {
    matches!(code, 400 | 422) && names_stream(&body.to_ascii_lowercase())
}

/// `stream` as a word, so a gateway's "upstream connect error" is not read
/// as a refusal and charged a second POST.
fn names_stream(body: &str) -> bool {
    body.match_indices("stream").any(|(at, _)| {
        !body[..at]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_')
    })
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
    // No settings file on this path, and `dev_flags::seed` is where the
    // exported timeout and reply cap are read (#273).
    crate::dev_flags::seed(&crate::settings::Settings::default());
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

/// How this request asks for its reply. The argument for `Stream` is #302,
/// and it has two halves.
///
/// A reply's first line is the Behavior name and runs one to three tokens,
/// so almost the whole wait is dialogue the buddy does not need in order to
/// start moving. And streaming is the only shape a dropped call can be
/// *stopped* in: closing a streaming connection ends the generation, where a
/// whole-reply request runs to completion on the server — and is billed —
/// whatever the client does, because there is no read to be between.
///
/// `Whole` is for the servers that will not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wire {
    Stream,
    Whole,
}

fn request_body(
    model: &str,
    session: &[Message],
    responses: bool,
    max_tokens: u32,
    wire: Wire,
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
    let mut body = if responses {
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
    };
    if wire == Wire::Stream {
        body["stream"] = serde_json::Value::Bool(true);
    }
    body
}

/// Ceiling on a streamed reply, in bytes.
///
/// `into_reader` is unlimited by default, where the whole-body read stops at
/// ureq's 10MB. A reply is two lines under a `max_tokens` cap of at most a
/// few hundred, so a megabyte is already far past anything a working server
/// sends; it is here to bound a broken one.
const STREAM_LIMIT: u64 = 1024 * 1024;

/// Why one attempt produced no reply.
enum Unsent {
    /// The server rejected the request for asking to stream, so the same
    /// question is worth one more send without the field — and because the
    /// answer is about the server rather than this call, it is worth
    /// remembering.
    NotStreamable(String),
    /// The stream broke before the server marked its end. Worth the same one
    /// retry, but a broken connection says nothing about whether the next
    /// stream will work, so it settles nothing.
    Cut(String),
    /// Superseded while the tokens were arriving. Nobody is waiting for this
    /// answer, so there is no error worth composing.
    Abandoned,
    /// A status, a transport error, or an unreadable reply.
    Failed(String),
}

impl Unsent {
    /// Borrowed, for the trace line that runs before the retry has decided
    /// anything.
    fn why(&self) -> &str {
        match self {
            Unsent::NotStreamable(why) | Unsent::Cut(why) | Unsent::Failed(why) => why,
            Unsent::Abandoned => "abandoned",
        }
    }

    /// Is the same question worth one send without the `stream` field, and
    /// does an answer settle whether this host streams at all?
    fn retry_settles(&self) -> Option<bool> {
        match self {
            Unsent::NotStreamable(_) => Some(true),
            Unsent::Cut(_) => Some(false),
            Unsent::Abandoned | Unsent::Failed(_) => None,
        }
    }

    fn into_error(self) -> String {
        self.why().to_string()
    }
}

/// How a streamed reply ended.
#[derive(Debug, PartialEq, Eq)]
enum Streamed {
    /// The server marked the end. Empty when the model spent its whole
    /// budget without writing anything.
    Complete(String),
    /// The body ended with the server never saying it was finished, so
    /// whatever arrived is half a sentence.
    ///
    /// ponytail: this trusts every OpenAI-compatible server to mark the end
    /// — `[DONE]`, a `finish_reason`, or `response.completed`. Measured on
    /// xAI (both paths) and oMLX, and it is what OpenAI's own stream does,
    /// so the untested servers in the README's table are expected to follow.
    /// One that does not still answers, because `post` retries it whole, but
    /// it looks truncated on every wake and so pays two POSTs forever without
    /// ever learning better; `AI_BUDDY_TRACE_DIRECTOR` names it in one line.
    /// The upgrade, if a real server ever turns up like this, is to keep what
    /// arrived rather than re-ask for it (#302).
    Cut,
    /// The body held no `data:` frame at all, so it was never an event
    /// stream: a server that took `stream` and ignored it. The refusal has
    /// no status of its own, which makes this the only place it shows.
    NotEventStream,
    /// Superseded, so the reader is dropped mid-generation (#302).
    Abandoned,
}

/// Assemble an SSE reply, giving up as soon as `abandoned` says the call is
/// no longer wanted.
///
/// Takes a `Read` rather than a response so the shapes below are checked
/// against canned bytes: this repo has no HTTP double, and a parser only a
/// live server can reach is a parser nobody checks.
fn read_stream(
    reader: impl std::io::Read,
    abandoned: impl Fn() -> bool,
) -> Result<Streamed, String> {
    use std::io::BufRead;

    let mut reader = std::io::BufReader::new(reader);
    let mut content = String::new();
    let mut line = String::new();
    let mut framed = false;
    let mut finished = false;
    loop {
        // Between frames, not between bytes: `read_line` parks until the
        // server says something, so a cancel lands one frame late — tens of
        // milliseconds once tokens are flowing, and time-to-first-token
        // before they are.
        if abandoned() {
            return Ok(Streamed::Abandoned);
        }
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(match (framed, finished) {
                (false, _) => Streamed::NotEventStream,
                (true, true) => Streamed::Complete(content),
                (true, false) => Streamed::Cut,
            });
        }
        let Some(payload) = line.trim().strip_prefix("data:") else {
            // A `:` comment holding a connection open, an event name, or the
            // blank line between frames. None of them carries text.
            continue;
        };
        framed = true;
        let payload = payload.trim();
        if payload == "[DONE]" {
            return Ok(Streamed::Complete(content));
        }
        let event = read_event(payload);
        finished |= event.finished;
        if let Some(delta) = event.delta {
            if content.is_empty() && !delta.is_empty() && tracing() {
                // The whole point of streaming, and the one moment worth a
                // line: a Behavior name is one to three tokens, so this is
                // roughly when the sprite could start moving (#302).
                eprintln!("director: first token");
            }
            content.push_str(&delta);
        }
    }
}

/// What one SSE event contributes. Named for the wire rather than the
/// animation `Frame` this codebase means everywhere else.
#[derive(Default)]
struct Event {
    /// Text it adds, if it adds any. Events that carry none — a role
    /// announcement, usage, a reasoning trace — are not errors.
    delta: Option<String>,
    /// It says the server is done, so an end of body after it is a whole
    /// reply rather than a connection cut.
    finished: bool,
}

/// Read one event in whichever of the two shapes `completions_url` chose.
///
/// chat-completions nests text under `choices` and marks the end with
/// `finish_reason`; Responses sends typed events whose `delta` *is* the text
/// and marks the end with `response.completed`. Both markers matter as much
/// as the text: `/v1/responses` ends the body without `[DONE]`, measured
/// against xAI, so the marker is the only thing that tells a finished reply
/// from a truncated one.
fn read_event(payload: &str) -> Event {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Event::default();
    };
    let choice = &value["choices"][0];
    if let Some(text) = choice["delta"]["content"].as_str() {
        return Event {
            delta: Some(text.to_string()),
            finished: choice["finish_reason"].is_string(),
        };
    }
    match value["type"].as_str() {
        Some("response.output_text.delta") => Event {
            delta: value["delta"].as_str().map(str::to_string),
            finished: false,
        },
        Some("response.completed") => Event {
            delta: None,
            finished: true,
        },
        _ => Event {
            delta: None,
            finished: choice["finish_reason"].is_string(),
        },
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

thread_local! {
    /// The abandon flag for the model call running on this thread.
    ///
    /// A thread-local rather than a field on `Endpoint`, because the socket
    /// lives in the worker's stack frame: "should this call stop" is a
    /// property of the thread, not of a Completer every wake shares. It also
    /// keeps the abort out of `Completer`, which `crates/core` could neither
    /// cause nor observe — cancellation is a property of a resource only the
    /// Shell holds.
    static ABANDONED: std::cell::RefCell<Option<Arc<AtomicBool>>> =
        const { std::cell::RefCell::new(None) };
}

/// Has the call running on this thread been dropped by the frame loop?
///
/// False on a thread that never carried one — the probe and the tests — so
/// `Endpoint` needs no second code path for them.
fn abandoned() -> bool {
    ABANDONED.with_borrow(|flag| {
        flag.as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
    })
}

/// Every session call the app has on the wire: one slot per Character Instance.
///
/// One registry rather than one per Instance. Sessions stay per-Instance inside
/// each `Endpoint` — ADR-0008 is untouched — and only the slot is centralised,
/// which is what makes a global concurrency cap expressible at all and gives
/// #18's spend panel somewhere to read. There is no cap: N Instances make N
/// calls, as they always have.
#[derive(Default)]
pub struct Slots {
    slots: HashMap<InstanceId, Slot>,
}

/// One Character Instance's place on the wire.
struct Slot {
    /// Which call is this Instance's current one. A reply stamped with any
    /// other number was computed for a moment the Instance has left.
    epoch: u64,
    tx: Sender<Delivered>,
    rx: Receiver<Delivered>,
    /// Raised when the call is superseded, and read by the worker between SSE
    /// frames, so an abandoned call closes its connection (#302).
    abandoned: Arc<AtomicBool>,
    waiting: bool,
    /// Whether the call answers something the user did, which is the whole of
    /// what the Thinking ellipsis asks.
    reactive: bool,
}

/// One worker's answer, stamped with the call it belongs to.
struct Delivered {
    epoch: u64,
    wake: Wake,
    context: Context,
}

impl Default for Slot {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            epoch: 0,
            tx,
            rx,
            abandoned: Arc::new(AtomicBool::new(false)),
            waiting: false,
            reactive: false,
        }
    }
}

impl Slot {
    /// Whatever this slot had on the wire stops being this Instance's answer.
    ///
    /// The epoch moves past it so `take` drops its reply, and its abandon flag
    /// rises so the worker closes the connection rather than generating on. The
    /// next call gets a fresh flag; the old one stays alive in the worker's
    /// hands.
    fn supersede(&mut self) {
        self.abandoned.store(true, Ordering::SeqCst);
        self.abandoned = Arc::new(AtomicBool::new(false));
        self.epoch += 1;
        self.waiting = false;
        self.reactive = false;
    }
}

/// The trace line for a proposed Behavior name nobody declared.
///
/// Carries the declared set because that is what makes the miss readable:
/// `prowll` beside `prowl` is a typo, beside `wave` it is a model ignoring
/// the contract (#243). Worker threads interleave, so the Instance id leads
/// the line as it does every other Director trace.
fn near_miss_line(id: &str, name: &str, behaviors: &[String]) -> String {
    format!(
        "director: {id} {name} is no declared Behavior; declared: {}",
        behaviors.join(", ")
    )
}

impl Slots {
    pub fn new() -> Self {
        Self::default()
    }

    /// Send this Character Prompt for `id`, abandoning whatever `id` had out.
    ///
    /// Infallible, because starting a call *is* the cancellation of the
    /// previous one: there is no busy to report and so no check for a caller
    /// to forget. Per-Instance newest-wins — "should this buddy's old Poke be
    /// abandoned for its new Throw" is always yes.
    pub fn wake<C: Completer + Send + Sync + 'static>(
        &mut self,
        id: &InstanceId,
        director: Arc<ModelDirector<C>>,
        context: Context,
    ) {
        let slot = self.slots.entry(id.clone()).or_default();
        slot.supersede();
        slot.waiting = true;
        slot.reactive = context.happened != Happened::Ambient;
        let epoch = slot.epoch;
        let tx = slot.tx.clone();
        let abandoned = Arc::clone(&slot.abandoned);
        let traced = id.clone();
        thread::spawn(move || {
            ABANDONED.with_borrow_mut(|flag| *flag = Some(abandoned));
            // Always send. A panic here would leave the slot waiting forever
            // and skip StaticDirector on every later tick.
            let wake = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let (wake, near_miss) = director.wake_and_near_miss(&context);
                // Traced here, beside the reply it came from, rather than in
                // the frame loop: the Wake reaching the frame loop is speech,
                // and speech is what a near miss is indistinguishable from.
                if tracing() {
                    if let Some(name) = near_miss {
                        eprintln!("{}", near_miss_line(&traced, &name, director.behaviors()));
                    }
                }
                wake
            }))
            .unwrap_or(Wake::Failed);
            let _ = tx.send(Delivered {
                epoch,
                wake,
                context,
            });
        });
    }

    /// The reply for `id`, with the Context it was computed for.
    ///
    /// The pair travels together because a proposal only means anything against
    /// the moment that asked for it, and a reply from a superseded moment is
    /// dropped here rather than handed out for a caller to compare — which is
    /// what stops a buddy saying "put me down" from the floor it landed on.
    pub fn take(&mut self, id: &InstanceId) -> Option<(Wake, Context)> {
        let slot = self.slots.get_mut(id)?;
        while let Ok(delivered) = slot.rx.try_recv() {
            if delivered.epoch != slot.epoch {
                continue;
            }
            slot.waiting = false;
            slot.reactive = false;
            return Some((delivered.wake, delivered.context));
        }
        None
    }

    /// Drop whatever `id` has on the wire, and forget the Instance.
    ///
    /// For the three moments where the answer would be the wrong buddy's: a
    /// Character switch, a Completer retarget, and a dismissal. Forgetting
    /// rather than emptying, so a registry that outlives its Instances does not
    /// accumulate them; the next `wake` opens a fresh slot.
    pub fn abandon(&mut self, id: &InstanceId) {
        if let Some(slot) = self.slots.remove(id) {
            slot.abandoned.store(true, Ordering::SeqCst);
        }
    }

    /// Whether `id` is waiting on the Director. Not a gate on `wake` — an
    /// observation, for the Static Director standing down while a session
    /// proposal is about to land.
    pub fn waiting(&self, id: &InstanceId) -> bool {
        self.slots.get(id).is_some_and(|slot| slot.waiting)
    }

    /// Whether what `id` is waiting on answers something the user did, which is
    /// the Thinking ellipsis's whole question: a proactive wake stays invisible.
    pub fn thinking(&self, id: &InstanceId) -> bool {
        self.slots
            .get(id)
            .is_some_and(|slot| slot.waiting && slot.reactive)
    }
}

/// Drop an in-flight wake and install a Completer for the new settings.
///
/// A Wake still on the wire would propose against the old host and session;
/// drop it and open a new turn. `Slots::abandon` closes the connection, so the
/// old host stops generating rather than merely going unheard.
pub fn retarget_model(
    slots: &mut Slots,
    id: &InstanceId,
    model: &mut Option<Arc<ModelDirector<Endpoint>>>,
    behaviors: impl IntoIterator<Item = impl Into<String>>,
    settings: &DirectorSettings,
    configured: bool,
) {
    slots.abandon(id);
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

    /// Run `body` with the three Director vars set as given and the switch
    /// cleared, then all four restored.
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
        with_vars(key, base, model, None, body)
    }

    /// Run `body` with `AI_BUDDY_DIRECTOR` exported as `value` and the other
    /// three cleared, so the developer's shell cannot decide the result.
    pub(crate) fn with_env_switch(value: &str, body: impl FnOnce()) {
        with_vars(None, None, None, Some(value), body)
    }

    fn with_vars(
        key: Option<&str>,
        base: Option<&str>,
        model: Option<&str>,
        enabled: Option<&str>,
        body: impl FnOnce(),
    ) {
        // Concurrent setenv/getenv is undefined behaviour. These vars are
        // process-global and the resolve tests share them; serialise mutation.
        static ENV: Mutex<()> = Mutex::new(());
        let _lock = ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        struct Guard(Vec<(&'static str, Option<String>)>);

        impl Drop for Guard {
            fn drop(&mut self) {
                // The Development variables are still cleared here, so this
                // leaves the live `dev_flags` values on the file defaults.
                // Seeding after the restore below would load the shell's
                // exports into them instead.
                crate::dev_flags::seed(&crate::settings::Settings::default());
                for (var, previous) in self.0.drain(..) {
                    apply(var, previous.as_deref());
                }
            }
        }

        fn apply(var: &str, value: Option<&str>) {
            match value {
                Some(value) => std::env::set_var(var, value),
                None => std::env::remove_var(var),
            }
        }

        // The four the caller sets, and every Development variable: a shell
        // that exported one of those would otherwise freeze a row or seed a
        // switch in a test that never mentions it (#273).
        //
        // The live `dev_flags` values those variables govern are
        // process-global too, so the lock owns them as well: seeded to the
        // defaults on the way in and again on the way out, no test has to
        // hand-restore them.
        let mut wanted = vec![
            (API_KEY, key),
            (BASE_URL, base),
            (MODEL, model),
            (ENABLED, enabled),
        ];
        wanted.extend(
            crate::dev_flags::test_vars()
                .into_iter()
                .map(|var| (var, None)),
        );

        let _guard = Guard(
            wanted
                .iter()
                .map(|(var, _)| (*var, std::env::var(var).ok()))
                .collect(),
        );
        for (var, value) in wanted {
            apply(var, value);
        }
        crate::dev_flags::seed(&crate::settings::Settings::default());
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

    /// One vocabulary, so no word that reads like on is quietly off.
    #[test]
    fn a_switch_variable_reads_one_vocabulary() {
        with_env(None, None, None, || {
            for (exported, want) in [
                (None, None),
                (Some("1"), Some(true)),
                (Some("on"), Some(true)),
                (Some("true"), Some(true)),
                (Some("yes"), Some(true)),
                (Some("0"), Some(false)),
                (Some("off"), Some(false)),
                (Some("false"), Some(false)),
                (Some("no"), Some(false)),
                (Some("ON"), Some(true)),
                (Some("Off"), Some(false)),
                // An expansion that produced nothing is a mistake, not a
                // choice, and a word no switch knows owns nothing.
                (Some(""), None),
                (Some("banana"), None),
            ] {
                match exported {
                    Some(value) => std::env::set_var(ENABLED, value),
                    None => std::env::remove_var(ENABLED),
                }
                assert_eq!(env_switch(ENABLED), want, "exported {exported:?}");
            }
            std::env::remove_var(ENABLED);
        });
    }

    /// A value nothing obeyed must not pass for one that was.
    #[test]
    fn an_unreadable_switch_value_is_named_at_launch() {
        with_env(None, None, None, || {
            std::env::set_var(ENABLED, "banana");
            let warnings = env_switch_warnings(&[ENABLED]);
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert!(warnings[0].contains(ENABLED), "{warnings:?}");
            assert!(warnings[0].contains("banana"), "{warnings:?}");

            std::env::set_var(ENABLED, "off");
            assert!(
                env_switch_warnings(&[ENABLED]).is_empty(),
                "a word the vocabulary knows is not a warning"
            );
            std::env::remove_var(ENABLED);
            assert!(
                env_switch_warnings(&[ENABLED]).is_empty(),
                "unset is silent"
            );
        });
    }

    /// The variable decides in both directions, which is the whole point of
    /// one vocabulary: an exported on lifts a file that says off.
    #[test]
    fn an_exported_switch_decides_either_way() {
        with_vars(None, None, None, Some("on"), || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let mut config = config_from(&settings);
            config.apply_switch(false);
            assert!(config.enabled, "the file said off, the process said on");
        });
    }

    /// On is still a request, not a Completer: the endpoint has to exist.
    #[test]
    fn an_exported_on_cannot_conjure_a_completer() {
        with_vars(None, None, None, Some("on"), || {
            let settings = resolve("https://api.openai.com", "gpt-4o-mini", None);
            let mut config = config_from(&settings);
            assert!(!config.configured, "a remote host with no key");
            config.apply_switch(true);
            assert!(!config.enabled);
        });
    }

    /// A word no switch knows leaves the decision where it was.
    #[test]
    fn an_unreadable_switch_value_leaves_the_file_deciding() {
        with_vars(None, None, None, Some("banana"), || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let mut config = config_from(&settings);
            config.apply_switch(true);
            assert!(config.enabled, "the file said on");
            config.apply_switch(false);
            assert!(!config.enabled, "the file said off");
        });
    }

    /// The README's promise that `off` "keeps Static even when a key is set".
    /// A local host is configured without a key, so nothing but the variable
    /// can hold the Director back.
    #[test]
    fn the_env_switch_vetoes_a_director_the_file_would_allow() {
        with_env_switch("off", || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let mut config = config_from(&settings);
            assert!(config.configured, "a local host needs no key");
            config.apply_switch(true);
            assert!(!config.enabled, "the file said on, the env vetoed it");
        });
    }

    #[test]
    fn the_saved_switch_decides_when_the_process_says_nothing() {
        with_env(None, None, None, || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let mut config = config_from(&settings);
            config.apply_switch(true);
            assert!(config.enabled);
            config.apply_switch(false);
            assert!(!config.enabled, "the file can always turn it off");
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

    fn local_endpoint() -> Endpoint {
        Endpoint {
            api_key: String::new(),
            url: "http://localhost:11434/v1/chat/completions".to_string(),
            model: "gemma4".to_string(),
            timeout: TIMEOUT,
            max_tokens: HOSTED_MAX_TOKENS,
            session: Mutex::new(Session::default()),
            streams: AtomicBool::new(true),
            agent: ureq::agent(),
        }
    }

    /// A streamed turn can end with no reply in more ways than a whole one
    /// could — abandoned, or cut off — and each has to leave the session as
    /// an error does (#302).
    #[test]
    fn a_turn_with_no_reply_leaves_no_half_answer_in_the_session() {
        let endpoint = local_endpoint();

        let (opening, asked) = endpoint.open_turn("hello");
        assert_eq!(asked.len(), 1, "the opening turn is the prompt alone");
        endpoint
            .close_turn(opening, Ok("stroll".to_string()))
            .unwrap();

        let (poked, _) = endpoint.open_turn("what just happened: poked");
        endpoint
            .close_turn(poked, Err("abandoned".to_string()))
            .unwrap_err();

        assert_eq!(
            spoken(&endpoint.open_turn("what just happened: thrown").1),
            [
                ("user", "hello"),
                ("assistant", "stroll"),
                ("user", "what just happened: thrown"),
            ]
        );
    }

    /// #312: a superseded call is still inside `post` when the wake that
    /// replaced it opens a turn on the same `Endpoint`. The loser must neither
    /// leave its question in the session nor take the winner's out.
    #[test]
    fn a_superseded_turn_neither_leaves_its_question_nor_takes_the_winners() {
        let endpoint = local_endpoint();
        let (opening, _) = endpoint.open_turn("hello");
        endpoint
            .close_turn(opening, Ok("stroll".to_string()))
            .unwrap();

        let (ambient, _) = endpoint.open_turn("what just happened: nothing");
        let (poked, asked) = endpoint.open_turn("what just happened: poked");
        assert_eq!(
            spoken(&asked),
            [
                ("user", "hello"),
                ("assistant", "stroll"),
                ("user", "what just happened: poked"),
            ],
            "the abandoned question must not be asked alongside the new one"
        );

        endpoint
            .close_turn(ambient, Err("abandoned".to_string()))
            .unwrap_err();
        endpoint.close_turn(poked, Ok("nap".to_string())).unwrap();

        assert_eq!(
            spoken(&endpoint.open_turn("what just happened: thrown").1),
            [
                ("user", "hello"),
                ("assistant", "stroll"),
                ("user", "what just happened: poked"),
                ("assistant", "nap"),
                ("user", "what just happened: thrown"),
            ]
        );
    }

    /// Nothing orders the two workers, so the superseded one may reach the
    /// session first and open its turn after the wake that replaced it. Whoever
    /// lands last, an answer must never be recorded against another turn's
    /// question — that is what the next Character Prompt is built from.
    #[test]
    fn an_answer_is_never_recorded_against_another_turns_question() {
        let endpoint = local_endpoint();
        let (poked, _) = endpoint.open_turn("what just happened: poked");
        let (ambient, _) = endpoint.open_turn("what just happened: nothing");

        endpoint.close_turn(poked, Ok("nap".to_string())).unwrap();
        endpoint
            .close_turn(ambient, Err("abandoned".to_string()))
            .unwrap_err();

        assert_eq!(
            spoken(&endpoint.open_turn("what just happened: thrown").1),
            [("user", "what just happened: thrown")],
            "a question whose turn is closed leaves nothing behind"
        );
    }

    fn spoken(session: &[Message]) -> Vec<(&str, &str)> {
        session
            .iter()
            .map(|message| (message.role, message.content.as_str()))
            .collect()
    }

    #[test]
    fn a_streamed_chat_completion_assembles_its_deltas() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"\\nhey\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(
            read_stream(std::io::Cursor::new(sse), || false).unwrap(),
            Streamed::Complete("stroll\nhey".to_string())
        );
    }

    #[test]
    fn a_streamed_responses_reply_assembles_its_deltas() {
        let sse = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"stroll\"}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"\\nhey\"}\n\n",
            "data: {\"type\":\"response.completed\"}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(
            read_stream(std::io::Cursor::new(sse), || false).unwrap(),
            Streamed::Complete("stroll\nhey".to_string())
        );
    }

    /// A frame arrives in as many TCP reads as the network feels like, and
    /// a keep-alive comment arrives between frames. Neither is a boundary
    /// the parser gets to see.
    #[test]
    fn a_frame_split_across_reads_is_still_one_event() {
        struct Dribble {
            bytes: Vec<u8>,
            sent: usize,
        }

        impl std::io::Read for Dribble {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let take = (self.bytes.len() - self.sent).min(3).min(buf.len());
                buf[..take].copy_from_slice(&self.bytes[self.sent..self.sent + take]);
                self.sent += take;
                Ok(take)
            }
        }

        let sse = concat!(
            ": keep-alive\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\"}}]}\n\n",
            ": keep-alive\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"\\nhey\"}}]}\n\n",
            "data: [DONE]\n\n",
        );
        let dribble = Dribble {
            bytes: sse.as_bytes().to_vec(),
            sent: 0,
        };
        assert_eq!(
            read_stream(dribble, || false).unwrap(),
            Streamed::Complete("stroll\nhey".to_string())
        );
    }

    /// A server that takes `stream: true` and answers with an ordinary body
    /// never says so in a status, so the absence of frames is the only signal
    /// there is — and it is the one worth another send. A stream that really
    /// did arrive empty is not: sending the same question again would spend a
    /// second call to be told the same nothing.
    #[test]
    fn a_body_with_no_frames_in_it_was_never_a_stream() {
        let whole = r#"{"choices":[{"message":{"content":"stroll\nhey"}}]}"#;
        assert_eq!(
            read_stream(std::io::Cursor::new(whole), || false).unwrap(),
            Streamed::NotEventStream
        );

        let spent = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"hmm\"}}]}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(
            read_stream(std::io::Cursor::new(spent), || false).unwrap(),
            Streamed::Complete(String::new()),
            "a model that thought its whole budget away did stream"
        );
    }

    /// xAI's `/v1/responses` ends the body with no `[DONE]` after it, so the
    /// end marker has to be enough on its own — and a body that stops with
    /// no marker at all is half a sentence, which must not reach the Speech
    /// bubble or the session (#302).
    #[test]
    fn a_marked_end_is_enough_and_an_unmarked_one_is_a_cut() {
        let responses = concat!(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"stroll\"}\n\n",
            "data: {\"type\":\"response.completed\"}\n\n",
        );
        assert_eq!(
            read_stream(std::io::Cursor::new(responses), || false).unwrap(),
            Streamed::Complete("stroll".to_string())
        );

        let completions = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        );
        assert_eq!(
            read_stream(std::io::Cursor::new(completions), || false).unwrap(),
            Streamed::Complete("stroll".to_string())
        );

        let cut = "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\\nhey th\"}}]}\n\n";
        assert_eq!(
            read_stream(std::io::Cursor::new(cut), || false).unwrap(),
            Streamed::Cut
        );
    }

    /// The load win. An endless stream is the only honest test of it: a
    /// reader that stopped on its own would prove nothing, and one that
    /// drains would hang this test rather than fail it.
    #[test]
    fn an_abandoned_stream_stops_reading_rather_than_draining() {
        struct Endless;

        impl std::io::Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let frame = b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n";
                buf[..frame.len()].copy_from_slice(frame);
                Ok(frame.len())
            }
        }

        let asked = std::cell::Cell::new(0);
        let abandoned = || {
            asked.set(asked.get() + 1);
            asked.get() > 3
        };
        assert_eq!(
            read_stream(Endless, abandoned).unwrap(),
            Streamed::Abandoned
        );
    }

    #[test]
    fn a_streaming_request_asks_for_a_stream_and_the_fallback_does_not() {
        let session = [Message {
            role: "user",
            content: "wave".to_string(),
        }];
        let streamed = request_body(
            "gpt-4o-mini",
            &session,
            false,
            HOSTED_MAX_TOKENS,
            Wire::Stream,
        );
        assert_eq!(streamed["stream"], true);
        let responses = request_body("grok-4.6", &session, true, HOSTED_MAX_TOKENS, Wire::Stream);
        assert_eq!(responses["stream"], true, "the Responses path streams too");

        let whole = request_body(
            "gpt-4o-mini",
            &session,
            false,
            HOSTED_MAX_TOKENS,
            Wire::Whole,
        );
        assert!(
            whole.get("stream").is_none(),
            "a retry must not name the field the server just refused"
        );
    }

    #[test]
    fn a_responses_request_uses_input_and_does_not_store() {
        let session = [Message {
            role: "user",
            content: "wave".to_string(),
        }];
        let body = request_body("grok-4.6", &session, true, HOSTED_MAX_TOKENS, Wire::Whole);
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
        let body = request_body("grok-4.6", &session, true, HOSTED_MAX_TOKENS, Wire::Whole);
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
            env_says: None,
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
            env_says: None,
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
            env_says: None,
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
    fn a_server_that_rejects_the_stream_field_earns_one_whole_retry() {
        assert!(refused_stream(
            400,
            r#"{"error":{"message":"Unrecognized request argument supplied: stream"}}"#
        ));
        assert!(
            refused_stream(
                422,
                r#"{"detail":[{"loc":["body","stream"],"msg":"extra fields not permitted"}]}"#
            ),
            "a strict server validates the body rather than the field"
        );
        assert!(
            !refused_stream(400, r#"{"error":{"message":"model gpt-9 does not exist"}}"#),
            "a 400 about anything else would fail the same way twice"
        );
        assert!(
            !refused_stream(403, "streaming is not available"),
            "403 is the key, the credits, or a path ACL; fallback_url owns that"
        );
        assert!(
            !refused_stream(400, "upstream connect error or disconnect/reset"),
            "a gateway saying upstream is not a server naming the stream field"
        );
        assert!(
            refused_stream(400, r#"{"error":"streaming is not supported here"}"#),
            "the word can still be inflected, it just cannot be a suffix"
        );
    }

    #[test]
    fn a_broken_stream_is_worth_a_retry_but_teaches_nothing() {
        assert_eq!(
            Unsent::NotStreamable("names the field".to_string()).retry_settles(),
            Some(true),
            "a host that rejected the field will reject it on the next wake too"
        );
        assert_eq!(
            Unsent::Cut("ended mid-reply".to_string()).retry_settles(),
            Some(false),
            "the answer is still owed, but one dropped body is no verdict on the host"
        );
        assert_eq!(
            Unsent::Failed("503".to_string()).retry_settles(),
            None,
            "dropping the stream field will not revive a server that is down"
        );
        assert_eq!(
            Unsent::Abandoned.retry_settles(),
            None,
            "nobody is waiting for a second attempt at a superseded call"
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

    /// The env keeps the last word over the file for these two, the same
    /// precedence `resolve` gives the endpoint (#272). Read here, decided in
    /// `dev_flags::seed`, so each export needs a re-seed to reach a read site.
    #[test]
    fn an_exported_limit_outranks_the_persisted_one() {
        with_env(None, None, None, || {
            let file = crate::settings::Settings {
                director_timeout_secs: "45".into(),
                director_max_tokens: "300".into(),
                ..Default::default()
            };
            crate::dev_flags::seed(&file);
            assert_eq!(timeout_for(false), Duration::from_secs(45));
            assert_eq!(max_tokens_for(false), 300);

            std::env::set_var(TIMEOUT_SECS, "7");
            std::env::set_var(MAX_TOKENS, "11");
            crate::dev_flags::seed(&file);
            assert_eq!(timeout_for(false), Duration::from_secs(7));
            assert_eq!(max_tokens_for(false), 11);
            std::env::remove_var(TIMEOUT_SECS);
            std::env::remove_var(MAX_TOKENS);
        });
    }

    /// Under the env lock because both functions read the live `dev_flags`
    /// values, which another test in this binary sets and clears.
    #[test]
    fn a_cold_local_model_gets_room_a_hosted_one_does_not_need() {
        with_env(None, None, None, || {
            assert!(timeout_for(true) > timeout_for(false));
            assert!(max_tokens_for(true) > max_tokens_for(false));
        });
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
            let mut slots = Slots::new();
            let id = "buddy".to_string();
            let saw = Arc::new(AtomicBool::new(false));
            slots.wake(
                &id,
                Arc::new(ModelDirector::new(
                    Watchful {
                        saw: Arc::clone(&saw),
                    },
                    ["stroll"],
                )),
                wake_context(),
            );
            let mut model = None;

            retarget_model(
                &mut slots,
                &id,
                &mut model,
                ["stroll"],
                &settings,
                config.configured,
            );

            assert!(
                waited_for(&saw),
                "the old host must be told to stop generating"
            );
            thread::sleep(Duration::from_millis(50));
            assert!(
                slots.take(&id).is_none(),
                "a Wake computed against the old target cannot answer the new one"
            );
            assert!(model.is_some());
        });
    }

    #[test]
    fn retarget_to_a_remote_without_a_key_leaves_static() {
        with_env(None, None, None, || {
            let settings = resolve("https://api.openai.com", "gpt-4o-mini", None);
            let config = config_from(&settings);
            let mut slots = Slots::new();
            let mut model = None;
            retarget_model(
                &mut slots,
                &"buddy".to_string(),
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
            let mut slots = Slots::new();
            let mut model = None;
            retarget_model(
                &mut slots,
                &"buddy".to_string(),
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

    /// The declared set is the half of the line #243 asks for: without it a
    /// reader cannot tell a typo from a model ignoring the contract.
    #[test]
    fn a_near_miss_line_names_the_instance_and_what_was_declared() {
        let line = near_miss_line(
            "buddy-1",
            "prowll",
            &["prowl".to_string(), "wave".to_string()],
        );

        assert_eq!(
            line,
            "director: buddy-1 prowll is no declared Behavior; declared: prowl, wave"
        );
    }

    /// A switch must not apply the old Character's reply, and must be able to
    /// start the new opening before that POST returns.
    #[test]
    fn abandon_drops_a_wake_that_still_arrives() {
        let mut slots = Slots::new();
        let id = "buddy".to_string();
        slots.wake(&id, answering("stroll", 40), wake_context());
        assert!(slots.waiting(&id), "the call is in flight");

        slots.abandon(&id);
        assert!(
            !slots.waiting(&id),
            "an abandoned call must not hold the next Character Prompt back"
        );
        thread::sleep(Duration::from_millis(80));
        assert!(
            slots.take(&id).is_none(),
            "the abandoned Wake must not land on the new Character"
        );
    }

    /// The ellipsis is for a turn the user is waiting on. An ambient wake is
    /// nobody's question, and showing it would tell the user the buddy is busy
    /// with them when it is not.
    #[test]
    fn only_a_reactive_call_is_thinking() {
        let mut slots = Slots::new();
        let (ambient, poked) = ("ambient".to_string(), "poked".to_string());

        slots.wake(&ambient, answering("stroll", 200), wake_context());
        slots.wake(
            &poked,
            answering("stroll", 200),
            Context {
                happened: Happened::Poke,
                ..wake_context()
            },
        );

        assert!(slots.waiting(&ambient) && !slots.thinking(&ambient));
        assert!(slots.thinking(&poked));
    }

    /// Stands in for the SSE loop: checks between frames, without a server.
    /// Spins far longer than a test should need.
    struct Watchful {
        saw: Arc<AtomicBool>,
    }

    impl Completer for Watchful {
        fn complete(&self, _: &str) -> Result<String, String> {
            for _ in 0..400 {
                if abandoned() {
                    self.saw.store(true, Ordering::SeqCst);
                    return Err("abandoned".to_string());
                }
                thread::sleep(Duration::from_millis(5));
            }
            Ok("idle".to_string())
        }
    }

    fn waited_for(flag: &AtomicBool) -> bool {
        for _ in 0..100 {
            if flag.load(Ordering::SeqCst) {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// A Completer that answers with a fixed Behavior name after a delay, so
    /// a test can tell one call apart from the one that superseded it.
    struct Answers {
        behavior: &'static str,
        delay: Duration,
    }

    impl Completer for Answers {
        fn complete(&self, _: &str) -> Result<String, String> {
            thread::sleep(self.delay);
            Ok(self.behavior.to_string())
        }
    }

    fn answering(behavior: &'static str, delay_ms: u64) -> Arc<ModelDirector<Answers>> {
        Arc::new(ModelDirector::new(
            Answers {
                behavior,
                delay: Duration::from_millis(delay_ms),
            },
            ["stroll", "nap"],
        ))
    }

    /// A registry with a call already out for `id`, long enough to still be
    /// there when the test acts. `pub(crate)` for the `settings` tests, which
    /// retarget through the same call.
    pub(crate) fn slots_awaiting_a_wake(id: &InstanceId) -> Slots {
        let mut slots = Slots::new();
        slots.wake(id, answering("stroll", 200), wake_context());
        slots
    }

    /// Poll the slot the way the frame loop does, until an answer lands.
    fn polled(slots: &mut Slots, id: &InstanceId) -> Option<(Wake, Context)> {
        for _ in 0..200 {
            if let Some(taken) = slots.take(id) {
                return Some(taken);
            }
            thread::sleep(Duration::from_millis(5));
        }
        None
    }

    fn behavior_of(wake: &Wake) -> &str {
        match wake {
            Wake::Proposed(proposal) => &proposal.behavior,
            Wake::Failed => "failed",
        }
    }

    /// The responsiveness this registry exists for: a Poke arriving while an
    /// ambient wake is still out sends its own prompt at once, and the answer
    /// the user gets is the one to what they just did.
    #[test]
    fn a_new_wake_supersedes_the_one_the_instance_had_on_the_wire() {
        let mut slots = Slots::new();
        let id = "buddy".to_string();

        slots.wake(&id, answering("stroll", 120), wake_context());
        slots.wake(&id, answering("nap", 0), wake_context());

        let (wake, _) = polled(&mut slots, &id).expect("the newest call answers");
        assert_eq!(behavior_of(&wake), "nap");

        thread::sleep(Duration::from_millis(200));
        assert!(
            slots.take(&id).is_none(),
            "the superseded reply must be dropped, not delivered a tick later"
        );
    }

    /// The Wake and its Context cannot be separated, so nothing downstream can
    /// read a proposal against a moment it was not computed for.
    #[test]
    fn take_hands_back_the_context_the_wake_was_computed_for() {
        let mut slots = Slots::new();
        let id = "buddy".to_string();
        let asked = Context {
            happened: Happened::Poke,
            standing: "Finder".to_string(),
            ..wake_context()
        };

        slots.wake(&id, answering("stroll", 0), asked);

        let (_, carried) = polled(&mut slots, &id).expect("the call answers");
        assert_eq!(carried.happened, Happened::Poke);
        assert_eq!(carried.standing, "Finder");
    }

    /// One registry, but the newest-wins latch is each Instance's own: two
    /// buddies poked at once are two conversations, per ADR-0008.
    #[test]
    fn one_instances_wake_leaves_anothers_slot_alone() {
        let mut slots = Slots::new();
        let (first, second) = ("first".to_string(), "second".to_string());

        slots.wake(&first, answering("stroll", 0), wake_context());
        slots.wake(&second, answering("nap", 0), wake_context());
        // Supersedes `first` only. `second` has said nothing about it.
        slots.wake(&first, answering("nap", 0), wake_context());

        let (theirs, _) = polled(&mut slots, &second).expect("the second buddy still answers");
        assert_eq!(behavior_of(&theirs), "nap");
        let (ours, _) = polled(&mut slots, &first).expect("the first buddy answers too");
        assert_eq!(behavior_of(&ours), "nap");
    }

    /// Superseding has to reach the worker, not just the epoch it answers on.
    /// Closing the connection is what stops a generation and gives the host its
    /// capacity back (#302), and the worker is the only thing holding the
    /// socket — so a flag it never reads buys nothing.
    #[test]
    fn superseding_raises_the_flag_the_worker_reads() {
        let saw = Arc::new(AtomicBool::new(false));
        let mut slots = Slots::new();
        let id = "buddy".to_string();

        slots.wake(
            &id,
            Arc::new(ModelDirector::new(
                Watchful {
                    saw: Arc::clone(&saw),
                },
                ["stroll"],
            )),
            wake_context(),
        );
        slots.wake(&id, answering("nap", 0), wake_context());

        assert!(
            waited_for(&saw),
            "the superseded worker ran on without ever seeing that it had been dropped"
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

        // For the exported timeout, as `run_probe` does.
        crate::dev_flags::seed(&crate::settings::Settings::default());
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
                happened: happened.clone(),
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
