//! The HTTP Completer, env config, and the in-flight session call.
//!
//! One of two Completers. `harness.rs` is the attached Harness over ACP; this
//! file is the chat-completions stand-in that stays for everyone who attaches
//! nothing. `AnyCompleter` picks between them once, at construction. ADR-0008.
//!
//! The Completer runs on a worker thread. The frame loop only polls `Slots`.
//! #18 binds these settings. Until then they come from the env.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ai_buddy_core::director::{
    self, Completer, Context, ModelDirector, Pace, Reply, Wake, WakeRequest, WAKE_EVERY,
};
use ai_buddy_core::roster::InstanceId;
use serde::Serialize;
use url::{Host, Url};

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
///
/// `pub(crate)` like the four above: the row it owns has to name it.
pub(crate) const WAKE_SECS: &str = "AI_BUDDY_DIRECTOR_WAKE_SECS";

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
/// the cap was the portable half of that fix; the other half is
/// `reasoning_effort`, which #612 now sends and `Field::Effort` guards for
/// the strict server this comment used to warn about.
///
/// 512 was not enough for every one of them at the model's default effort.
/// Measured against a Character Prompt carrying the shipped cat personality,
/// `gpt-oss-20b-MXFP4-Q8` spent the whole budget thinking on about 40% of
/// wakes and returned empty content — see
/// `measure_the_reply_contract_failure_rate`, which found the same 40% end
/// to end. With `reasoning_effort: "low"` the same model thinks about a
/// tenth as much, so the cap is no longer the number under pressure and is
/// left where it is.
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
    /// The attached Harness, when `AI_BUDDY_HARNESS` named one. Its `login`
    /// is the third Chat state ADR-0010 names: attached, not authenticated.
    pub harness: Option<crate::harness::HarnessInspect>,
    /// The HTTP Completer in force: the model, and the host without its
    /// scheme, path or userinfo. What the Chat header names when no Harness
    /// does (#474). Never the key — ADR-0010's seventh rule covers drawing a
    /// credential as firmly as logging one, and userinfo is one.
    pub model: String,
    pub host: String,
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

    /// `settings` because the switch and the endpoint are read from different
    /// places and the Chat header needs both: the config says whether anything
    /// answers, the settings say what would.
    pub fn inspect(&self, settings: &DirectorSettings) -> DirectorInspect {
        DirectorInspect {
            enabled: self.enabled,
            configured: self.configured,
            ambient_wakes: self.ambient_allowed,
            wake_secs: self.ambient_first.as_secs(),
            last_payload: None,
            harness: crate::harness::attached().map(|session| session.inspect()),
            model: settings.model.clone(),
            host: host_of(&settings.base_url),
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
    let configured = crate::harness::attached().is_some()
        || !settings.api_key.is_empty()
        || is_local(&settings.base_url);
    let env_says = env_switch(ENABLED);
    DirectorConfig {
        enabled: env_says.unwrap_or(true) && configured,
        configured,
        env_says,
        key_invalid: settings.key_invalid,
        wake_every: WAKE_EVERY,
        ambient_first: ambient_first(),
        ambient_allowed: true,
    }
}

/// Whichever Completer this process has: the attached Harness for every
/// Instance (ADR-0008: one session), else an HTTP `Endpoint` per Instance.
///
/// An enum rather than `Box<dyn Completer>` so `Endpoint`'s inherent methods
/// (`url`, `origin`, the probe) keep their type.
pub enum AnyCompleter {
    Http(Endpoint),
    Harness(Arc<crate::harness::Session>),
}

impl Completer for AnyCompleter {
    fn complete(&self, request: &WakeRequest) -> Result<Reply, String> {
        match self {
            AnyCompleter::Http(endpoint) => endpoint.complete(request),
            AnyCompleter::Harness(session) => session.complete(request),
        }
    }
}

/// The Completer `configured` promises. Harness first: with one attached the
/// HTTP settings are not consulted at all.
pub fn completer_from(settings: &DirectorSettings) -> Option<AnyCompleter> {
    match crate::harness::attached() {
        Some(session) => Some(AnyCompleter::Harness(session)),
        None => endpoint_from(settings).map(AnyCompleter::Http),
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
        takes_effort: AtomicBool::new(true),
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
    let mut lines = crate::harness::startup_lines(config.enabled);
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

/// The first ambient wait in force.
///
/// As with the timeout, the variable-or-file decision is `dev_flags::seed`'s,
/// so this is a read rather than a second place precedence is settled. Zero
/// and unparsable are unset there: a buddy waking every no seconds is not a
/// value to keep (#262).
fn ambient_first() -> Duration {
    crate::dev_flags::director_wake_secs().map_or(Pace::FIRST, Duration::from_secs)
}

/// The host and port a base URL points at, with the scheme, the path and any
/// userinfo dropped. Empty when the value is not a URL with a host.
///
/// `Url` rather than splitting on `://`, `/` and `@` by hand: the two callers
/// are a security check and a label, and in `10.0.0.1@172.16.evil.com` the
/// digits belong to the credentials while the request goes to evil.com. A
/// parser that already knows that is the one to ask. A password written into a
/// URL is also a credential this must not hand back to a caller that draws it
/// (#474), and `host_str` never carries one.
///
/// Parse a base URL and validate it has a host. Returns `None` if parsing fails
/// or the URL has no host (including scheme-less inputs).
fn url_of(base: &str) -> Option<Url> {
    Url::parse(base).ok().filter(Url::has_host)
}

/// Extract host and port from a base URL, or empty string if none.
pub fn host_of(base: &str) -> String {
    let Some(url) = url_of(base) else {
        return String::new();
    };
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

/// Is this base URL served from this machine or this LAN?
///
/// A local host (loopback, RFC1918, unique-local IPv6, or `.local`) makes
/// `AI_BUDDY_DIRECTOR_API_KEY` optional. A remote host requires a real key.
fn is_local(base: &str) -> bool {
    let Some(url) = url_of(base) else {
        return false;
    };
    match url.host() {
        // A fully-qualified name ends in a dot, and DNS reads it as the same
        // name. `Url` has already lowercased it.
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.');
            name == "localhost" || name.ends_with(".local")
        }
        Some(Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private(),
        // fc00::/7 is the IPv6 private range. `Ipv6Addr::is_unique_local` is
        // still unstable, and this repo builds on the pinned stable toolchain.
        Some(Host::Ipv6(ip)) => ip.is_loopback() || ip.octets()[0] & 0xfe == 0xfc,
        None => false,
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

/// What an empty wake-interval field means, in seconds. One default here:
/// the first wait does not depend on where the Completer runs.
pub(crate) fn wake_secs_placeholder() -> String {
    Pace::FIRST.as_secs().to_string()
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

/// Whether this URL is served by xAI, which decides the inference path below.
fn host_is_xai(url: &str) -> bool {
    url_of(url)
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| host == "api.x.ai" || host.ends_with(".api.x.ai"))
}

/// Whether this URL already points at the Responses path.
///
/// The parsed path, not a substring of the whole URL: a query that merely
/// mentions `/responses` is not the path being called.
fn uses_responses(url: &str) -> bool {
    url_of(url).is_some_and(|url| url.path().contains("/responses"))
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
    /// Does this host take `reasoning_effort`? Same shape as `streams`, and
    /// same reason: it starts optimistic, and only a server that names the
    /// field in a rejection ever turns it off (#612).
    takes_effort: AtomicBool,
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
    /// Asks for a stream and for low reasoning effort, and gives up either
    /// one for a server that names it in a rejection. A fallback retries the
    /// same session snapshot, so every attempt asks the same question and
    /// only one answer is ever recorded — and once a request without the
    /// field has succeeded where the request with it did not, this endpoint
    /// stops asking, rather than paying two POSTs on every wake for the rest
    /// of the session.
    pub fn post(&self, url: &str, prompt: &str) -> Result<Reply, String> {
        let (turn, snapshot) = self.open_turn(prompt);
        let mut wire = if self.streams.load(Ordering::SeqCst) {
            Wire::Stream
        } else {
            Wire::Whole
        };
        let mut effort = self.takes_effort.load(Ordering::SeqCst);
        let mut reply = self.send(url, &snapshot, wire, effort);
        // A loop rather than one retry: there are two optional fields now,
        // and a validator strict enough to refuse both would otherwise lose
        // the wake. Each pass drops exactly the field the rejection named.
        // Bounded by the count of those fields rather than by trusting the
        // body to stop naming one, because the cost of being wrong is a
        // worker thread posting for ever.
        for _ in 0..Field::ALL.len() {
            let Err(unsent) = &reply else { break };
            let Some((field, settles)) = unsent.retry_settles() else {
                break;
            };
            let sent = match field {
                Field::Stream => wire == Wire::Stream,
                Field::Effort => effort,
            };
            if !sent {
                break;
            }
            if tracing() {
                eprintln!(
                    "director: {}; retrying without {}",
                    unsent.why(),
                    field.name()
                );
            }
            // A call dropped between two attempts must not become a fresh
            // request the frame loop can no longer reach.
            if abandoned() {
                reply = Err(Unsent::Abandoned);
                break;
            }
            match field {
                Field::Stream => wire = Wire::Whole,
                Field::Effort => effort = false,
            }
            reply = self.send(url, &snapshot, wire, effort);
            // Evidence, not a guess: the server rejected the field and the
            // request without it worked, so this host does not take it. A
            // refusal misread from some unrelated 400 fails twice and settles
            // nothing, and neither does a stream that merely broke.
            if reply.is_ok() && settles {
                match field {
                    Field::Stream => self.streams.store(false, Ordering::SeqCst),
                    Field::Effort => self.takes_effort.store(false, Ordering::SeqCst),
                }
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
    fn close_turn(&self, turn: u64, reply: Result<Reply, String>) -> Result<Reply, String> {
        let mut session = self.session.lock().expect("session lock");
        if session.opened != turn {
            return reply;
        }
        match reply {
            Ok(reply) => {
                // What arrived, truncated or not: the user heard these words,
                // so the next turn is built on the same ones they heard — and
                // when the cap ended it, the session says so, so the model
                // reading its own last turn back sees where it was stopped
                // rather than a sentence it appears to have abandoned (#610).
                //
                // Appended here and nowhere earlier: `parse_proposal` reads
                // the first line that is a whole Behavior name and says the
                // rest, so a mark in the text the parser sees would be spoken
                // as the buddy's own line.
                session.messages.push(Message {
                    role: "assistant",
                    content: director::marked(&reply.text, reply.truncated),
                });
                Ok(reply)
            }
            Err(error) => {
                session.messages.pop();
                Err(error)
            }
        }
    }

    /// One POST. Both attempts come through here, so the fallback differs
    /// from the first try in exactly one field.
    fn send(
        &self,
        url: &str,
        session: &[Message],
        wire: Wire,
        effort: bool,
    ) -> Result<Reply, Unsent> {
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
            effort,
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
            // Only a field this request actually sent: a body that names one
            // we left out is talking about something else, and dropping it
            // again would send the identical request.
            let refused = refused_field(code, &text).filter(|field| match field {
                Field::Stream => wire == Wire::Stream,
                Field::Effort => effort,
            });
            return Err(match refused {
                Some(field) => Unsent::Refused(field, error),
                None => Unsent::Failed(error),
            });
        }

        match wire {
            Wire::Whole => {
                let (_, text) = read_response(response).map_err(Unsent::Failed)?;
                // The same three endings as a stream, in the shape a server
                // that will not stream sends them.
                let said = content_from_body(&text);
                match (said, truncated_body(&text)) {
                    (Ok(said), true) if !said.trim().is_empty() => Ok(Reply::truncated(said)),
                    (Ok(said), false) => Ok(Reply::whole(said)),
                    (_, true) => Err(Unsent::Truncated(self.out_of_budget(url, true))),
                    (Err(error), false) => Err(Unsent::Failed(format!("{url}: {error}"))),
                }
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
                match read_stream(reader, abandoned, think) {
                    Ok(streamed) => self.reply_from(url, streamed),
                    Err(error) => Err(Unsent::Failed(format!("{url}: {error}"))),
                }
            }
        }
    }

    /// What a finished stream is worth. Apart from the socket, so all five
    /// endings are checked without one.
    fn reply_from(&self, url: &str, streamed: Streamed) -> Result<Reply, Unsent> {
        match streamed {
            Streamed::Complete(content) if !content.trim().is_empty() => Ok(Reply::whole(content)),
            Streamed::Complete(_) => Err(Unsent::Failed(format!(
                "{url}: streamed reply had no text content"
            ))),
            // Best effort, because we are the ones who cut it off: whatever
            // the model wrote before the cap is parsed and shown, marked as
            // truncated where it is drawn. The turn is still a failure on the
            // wire — never retried, and the Action Log names the cap (#610).
            Streamed::Truncated(content) if !content.trim().is_empty() => {
                Ok(Reply::truncated(content))
            }
            // Best effort with nothing in hand is silence. `StaticDirector`
            // takes the turn, as it does for any wake with no words.
            Streamed::Truncated(_) => Err(Unsent::Truncated(self.out_of_budget(url, true))),
            Streamed::Cut => Err(Unsent::Cut(format!("{url}: the stream ended mid-reply"))),
            Streamed::NotEventStream => Err(Unsent::Refused(
                Field::Stream,
                format!("{url}: answered 200 with no event stream in it"),
            )),
            Streamed::Abandoned => Err(Unsent::Abandoned),
        }
    }

    /// Why the turn produced nothing, in the words that name the knob. The
    /// Action Log writes this line, so it has to answer "why did the buddy go
    /// quiet" on its own: the model, the cap it hit, and the setting that
    /// moves it (#610).
    ///
    /// Both halves of a truncation are refused, and both are worth telling
    /// apart when reading the log: a model that thought its budget away and
    /// one that ran out mid-sentence call for different settings.
    fn out_of_budget(&self, url: &str, silent: bool) -> String {
        let cap = self.max_tokens;
        let what = if silent {
            format!("spent all {cap} tokens thinking and wrote no reply")
        } else {
            format!("was cut off mid-reply by the {cap}-token cap")
        };
        format!(
            "{url}: {} {what}; raise {MAX_TOKENS} or lower the model's reasoning effort",
            self.model
        )
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

/// One Action Log pair for an HTTP Completer wake: `prompt` then `turn`.
///
/// One pair per `complete`, including after a fallback POST — that is still
/// one wake. `chars` rather than the Character Prompt body (#435).
pub(crate) fn note_http_call(
    dir: &std::path::Path,
    request: &WakeRequest,
    result: Result<&Reply, &str>,
    // `truncated`: why a turn that hit the cap stopped, in the same words the
    // refusal uses. Read only when the reply is marked.
    truncated: &str,
) {
    crate::action_log::append(
        dir,
        "prompt",
        serde_json::json!({
            "instance": request.instance,
            "wake": if request.reactive { "reactive" } else { "proactive" },
            "chars": request.prompt.len(),
        }),
    );
    match result {
        // A truncated turn is a failure of the wire that still produced words:
        // the line carries both, so the log says what was shown and why there
        // was no more of it (#610).
        Ok(reply) if reply.truncated => crate::action_log::append(
            dir,
            "turn",
            serde_json::json!({ "text": reply.text, "truncated": truncated }),
        ),
        Ok(reply) => {
            crate::action_log::append(dir, "turn", serde_json::json!({ "text": reply.text }))
        }
        Err(why) => crate::action_log::append(dir, "turn", serde_json::json!({ "error": why })),
    }
}

impl Completer for Endpoint {
    fn complete(&self, request: &WakeRequest) -> Result<Reply, String> {
        let prompt = &request.prompt;
        if tracing() {
            eprintln!("director: sending POST {} model={}", self.url, self.model);
            trace_block("prompt", prompt);
            eprintln!("director: waiting for model");
        }
        let result = match self.post(&self.url, prompt) {
            Ok(reply) => {
                if tracing() {
                    trace_block("model", &reply.text);
                }
                Ok(reply)
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
                        Ok(reply) => {
                            if tracing() {
                                trace_block("model", &reply.text);
                            }
                            Ok(reply)
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
        };
        let noted = match &result {
            Ok(reply) => Ok(reply),
            Err(why) => Err(why.as_str()),
        };
        note_http_call(
            &ai_buddy_core::memory::data_dir(),
            request,
            noted,
            &self.out_of_budget(&self.url, false),
        );
        result
    }
}

/// Scheme, host and port, with the path dropped — what `/v1/models` is hung
/// off for the pre-flight probe, and what a trace line names the endpoint by.
fn origin(url: &str) -> String {
    url_of(url).map_or_else(|| url.to_string(), |url| url.origin().ascii_serialization())
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

/// Which optional field, if any, did the server reject the request *for*?
///
/// 400 and 422 are the codes that mean "your body is wrong", and a strict
/// OpenAI-compatible server names the field it did not recognise. Nothing
/// else counts: a 401 or 403 would fail the same way without the field, and
/// on the xAI paths 403 already means something `fallback_url` handles. The
/// cost of reading this too narrowly is one turn of `StaticDirector`.
///
/// `Effort` before `Stream`, because a validator that lists every unknown
/// key names both while the caller drops one field per attempt: the field
/// #612 added is the one to give up first.
fn refused_field(code: u16, body: &str) -> Option<Field> {
    if !matches!(code, 400 | 422) {
        return None;
    }
    let body = body.to_ascii_lowercase();
    Field::ALL
        .into_iter()
        .find(|field| names(&body, field.name()))
}

/// The field as a word, so a gateway's "upstream connect error" is not read
/// as a refusal of `stream` and charged a second POST.
fn names(body: &str, field: &str) -> bool {
    body.match_indices(field).any(|(at, _)| {
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
    if let Some(session) = crate::harness::attached() {
        session.spawn_preflight();
        return;
    }
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
        Ok(reply) => {
            println!("  ok {}", clip_body(&reply.text));
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
    effort: bool,
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
        let mut chat = serde_json::json!({
            "model": model,
            "messages": input,
            "max_tokens": max_tokens,
        });
        // The Responses branch above has asked for low effort since #302,
        // for the same reason: a two-line Behavior pick is not worth a long
        // think. Measured on chat-completions in #597 — gpt-oss-20b under
        // the real Character Prompt returned 7 empty `length` finishes in 20
        // runs at a 512-token cap, and 0 in 20 with this field. It is not a
        // field every server accepts, which is what `Field::Effort` guards.
        if effort {
            chat["reasoning_effort"] = serde_json::Value::String("low".to_string());
        }
        chat
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

/// An optional request field a server may refuse the whole request over.
///
/// Both are the same bet: worth sending where it works, never worth losing a
/// wake to. One `Endpoint` flag apiece remembers the answer, so a host that
/// refuses one pays the extra POST once per session and not once per wake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    /// `stream` (#302).
    Stream,
    /// `reasoning_effort` on chat-completions (#612). The Responses branch
    /// spells the same ask as `reasoning.effort` and is not guarded here:
    /// xAI is the only host that takes that path, and it accepts it.
    Effort,
}

impl Field {
    /// Every guarded field, in the order a retry gives them up. One list, so
    /// the rejection reader and the retry bound cannot disagree about how
    /// many there are.
    const ALL: [Field; 2] = [Field::Effort, Field::Stream];

    /// The name in the body, which is also the name a rejection uses.
    fn name(self) -> &'static str {
        match self {
            Field::Stream => "stream",
            Field::Effort => "reasoning_effort",
        }
    }
}

/// Why one attempt produced no reply.
enum Unsent {
    /// The server rejected the request for naming this field, so the same
    /// question is worth one more send without it — and because the answer
    /// is about the server rather than this call, it is worth remembering.
    Refused(Field, String),
    /// The stream broke before the server marked its end. Worth the same one
    /// retry, but a broken connection says nothing about whether the next
    /// stream will work, so it settles nothing.
    Cut(String),
    /// The model reached the token cap, so the turn ran out of budget rather
    /// than failing. Never a reply and never worth a retry: the same question
    /// at the same cap gets the same nothing. The text names the cap and the
    /// model, because the cap is the only knob that changes the answer, and
    /// says whether the budget went on thinking or on half a sentence (#610).
    Truncated(String),
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
            Unsent::Refused(_, why)
            | Unsent::Cut(why)
            | Unsent::Truncated(why)
            | Unsent::Failed(why) => why,
            Unsent::Abandoned => "abandoned",
        }
    }

    /// Which field is the same question worth one more send without, and
    /// does an answer settle whether this host takes that field at all?
    fn retry_settles(&self) -> Option<(Field, bool)> {
        match self {
            Unsent::Refused(field, _) => Some((*field, true)),
            // A broken connection says nothing about whether the next stream
            // will work, so the retry is owed and the verdict is not.
            Unsent::Cut(_) => Some((Field::Stream, false)),
            // The same question at the same cap gets the same nothing, so a
            // truncation is not worth a second POST with any field dropped.
            Unsent::Abandoned | Unsent::Truncated(_) | Unsent::Failed(_) => None,
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
    /// The server marked the end *and* said the token cap is why:
    /// `finish_reason: "length"` on chat-completions, `response.incomplete`
    /// on Responses. Whatever text arrived is kept, because it is still what
    /// the model said and is still shown: empty means the budget went on
    /// thinking and there is nothing to show, and half a sentence means it
    /// ran out writing and that sentence is the reply (#610).
    Truncated(String),
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
/// no longer wanted, and handing every marked thought to `thought` on the way.
///
/// Takes a `Read` rather than a response so the shapes below are checked
/// against canned bytes: this repo has no HTTP double, and a parser only a
/// live server can reach is a parser nobody checks. `thought` is a parameter
/// for the same reason: the Shell's door is a process global, and a routing
/// only the app can reach is a routing nobody checks.
fn read_stream(
    reader: impl std::io::Read,
    abandoned: impl Fn() -> bool,
    thought: impl Fn(&str),
) -> Result<Streamed, String> {
    let mut thinking = String::new();
    let ended = read_frames(reader, abandoned, &thought, &mut thinking);
    // The Chat surface keeps no thought of its own, so the last line stays on
    // screen until it is told the turn that wrote it has ended (ADR-0025).
    // Asked the same way the draw was, so a stream whose thinking was all
    // whitespace takes away nothing, having drawn nothing.
    if crate::acp_wire::thinking_line(&thinking).is_some() {
        thought("");
    }
    ended
}

/// The frame loop itself, split out so that every way it can end — a marker,
/// a cut body, an abandon, an I/O error — leaves through the one line above
/// that takes the strip away.
fn read_frames(
    reader: impl std::io::Read,
    abandoned: impl Fn() -> bool,
    thought: &impl Fn(&str),
    thinking: &mut String,
) -> Result<Streamed, String> {
    use std::io::BufRead;

    let mut reader = std::io::BufReader::new(reader);
    let mut content = String::new();
    let mut line = String::new();
    let mut framed = false;
    let mut finished = false;
    let mut truncated = false;
    let ended = |content: String, truncated: bool| {
        if truncated {
            Streamed::Truncated(content)
        } else {
            Streamed::Complete(content)
        }
    };
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
                (true, true) => ended(content, truncated),
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
            return Ok(ended(content, truncated));
        }
        let event = read_event(payload);
        finished |= event.finished;
        truncated |= event.truncated;
        if let Some(chunk) = event.thought {
            thinking.push_str(&chunk);
            // The line being written now, by the same rule the ACP lane
            // draws: a chunk lands mid-sentence, and half a sentence on its
            // own reads as nonsense.
            if let Some(line) = crate::acp_wire::thinking_line(thinking) {
                thought(line);
            }
        }
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
    /// announcement, usage, an end marker — are not errors.
    delta: Option<String>,
    /// Thinking it adds, if the server marked any as thinking. Never `delta`:
    /// that is the reply, whose first line has to parse as a Behavior name and
    /// whose rest the buddy says out loud (ADR-0025).
    thought: Option<String>,
    /// It says the server is done, so an end of body after it is a whole
    /// reply rather than a connection cut.
    finished: bool,
    /// The reason it is done is the token cap, not the model having said
    /// what it had to say.
    truncated: bool,
}

/// Read one event in whichever of the two shapes `completions_url` chose.
///
/// chat-completions nests text under `choices` and marks the end with
/// `finish_reason`; Responses sends typed events whose `delta` *is* the text
/// and marks the end with `response.completed`. Both markers matter as much
/// as the text: `/v1/responses` ends the body without `[DONE]`, measured
/// against xAI, so the marker is the only thing that tells a finished reply
/// from a truncated one.
///
/// The *value* of the marker matters too. `length` and `response.incomplete`
/// both mean the token cap ended the turn rather than the model, which is a
/// different thing from a reply, and only these two fields say which happened
/// (#610).
fn read_event(payload: &str) -> Event {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Event::default();
    };
    let choice = &value["choices"][0];
    let chunk = &choice["delta"];
    let kind = value["type"].as_str();
    let finish = choice["finish_reason"].as_str();
    // A Responses event's `delta` *is* the text, so its `type` is the only
    // thing that says which text it is.
    let typed = |name| {
        if kind == Some(name) {
            value["delta"].as_str()
        } else {
            None
        }
    };
    Event {
        delta: chunk["content"]
            .as_str()
            .or_else(|| typed("response.output_text.delta"))
            .map(str::to_string),
        // Two names for one field, both in use and neither in OpenAI's
        // schema: `reasoning_content` on llama.cpp, oMLX, SGLang and LM
        // Studio for R1, `reasoning` on vLLM since its rename, Ollama and LM
        // Studio for gpt-oss. Reading one name misses the other
        // (`docs/research/reasoning-versus-the-final-answer.md` §2.4).
        //
        // Responses types its reasoning apart from its answer, and the summary
        // is the half a client is meant to read: the raw
        // `response.reasoning_text.delta` is a second stream, and reading both
        // into one line would interleave two texts (§2.2).
        thought: chunk["reasoning_content"]
            .as_str()
            .or_else(|| chunk["reasoning"].as_str())
            .or_else(|| typed("response.reasoning_summary_text.delta"))
            .map(str::to_string),
        // Responses ends a capped reply with `response.incomplete` and no
        // `[DONE]` after it, so without that name the body simply stopped and
        // the turn was re-asked whole.
        finished: finish.is_some()
            || matches!(kind, Some("response.completed" | "response.incomplete")),
        // `length` is the only reason the spec gives for a cap; every other
        // value — `stop`, `tool_calls`, `content_filter` — is a reply the
        // server chose to end, and is left alone.
        truncated: finish == Some("length") || kind == Some("response.incomplete"),
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

/// Did this whole body end at the token cap? The same two markers the stream
/// carries, in the shape a non-streamed reply puts them: `finish_reason` on
/// chat-completions, `incomplete_details.reason` on Responses. Asked of every
/// whole body, because the answer decides both halves: with text it marks the
/// reply, and with none it names the cap instead of "no text content" (#610).
fn truncated_body(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    value["choices"][0]["finish_reason"] == "length"
        || value["incomplete_details"]["reason"] == "max_output_tokens"
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

/// The Shell's thought door, set once at startup.
///
/// The Harness lane reaches the Chat surface through `Forwarded::Thought`,
/// which it is handed when it is attached. The HTTP lane is built from
/// settings and holds no window handle, so it is given the door instead of a
/// route to it — and a `OnceLock` rather than a field on `Endpoint`, because a
/// Retarget rebuilds the Completer and would drop a field (#611).
///
/// Unset in the probe and the tests, which have no Chat surface to draw on.
///
/// ponytail: one door for every Instance, so two Instances thinking at once
/// overwrite each other's line and the first to finish takes the strip away.
/// The unattributed half is ADR-0025's own decision and holds on both lanes:
/// the strip has no Instance to address. The overlap is this lane's alone —
/// `Session::turn` holds a lock, so one Harness child serves one turn at a
/// time (ADR-0008), while `Slots` gives every Instance its own thread and its
/// own endpoint. A thought is worth reading only while it is being thought,
/// which is why this is left. The upgrade, if two buddies on the wire at once
/// ever becomes the common case, is to name the Instance on the event and let
/// the surface pick.
static THOUGHT: std::sync::OnceLock<Box<dyn Fn(String) + Send + Sync>> = std::sync::OnceLock::new();

/// Hand the Completer lane the door to every open Chat surface. The first
/// door wins and a later one is dropped: the Shell opens exactly one, and a
/// second caller would be a test racing the app it is testing.
pub fn on_thought(door: Box<dyn Fn(String) + Send + Sync>) {
    let _ = THOUGHT.set(door);
}

/// Draw `line` as what the Completer is thinking right now, or take the strip
/// away when it is empty. Nothing keeps it (ADR-0025).
fn think(line: &str) {
    if let Some(door) = THOUGHT.get() {
        door(line.to_string());
    }
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
    answered: Answered,
}

/// What one wake came back with.
///
/// The three travel together because each is meaningless without the others: a
/// proposal only means anything against the moment that asked for it, and the
/// near miss is the only thing that tells a proposal-shaped reply naming an
/// undeclared Behavior apart from a model that chose to talk (#243).
pub struct Answered {
    pub wake: Wake,
    pub context: Context,
    /// The Behavior name the reply proposed that this Character declares none
    /// of. `None` on every other reply.
    pub near_miss: Option<String>,
    /// The cap ended this turn, so what was said is as far as the model got.
    /// The line the Chat surface remembers is marked with it (#610).
    pub truncated: bool,
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
        slot.reactive = director::reactive(&context.happened);
        let epoch = slot.epoch;
        let tx = slot.tx.clone();
        let abandoned = Arc::clone(&slot.abandoned);
        let traced = id.clone();
        thread::spawn(move || {
            ABANDONED.with_borrow_mut(|flag| *flag = Some(abandoned));
            // Always send. A panic here would leave the slot waiting forever
            // and skip StaticDirector on every later tick.
            let woken = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let woken = director.wake_and_near_miss(&context);
                // Traced here, beside the reply it came from. The Action Log
                // takes it from `take` instead, where a superseded reply has
                // already been dropped.
                if tracing() {
                    if let Some(name) = &woken.near_miss {
                        eprintln!("{}", near_miss_line(&traced, name, director.behaviors()));
                    }
                }
                woken
            }))
            .unwrap_or(director::Woken {
                wake: Wake::Failed,
                near_miss: None,
                truncated: false,
            });
            let _ = tx.send(Delivered {
                epoch,
                answered: Answered {
                    wake: woken.wake,
                    context,
                    near_miss: woken.near_miss,
                    truncated: woken.truncated,
                },
            });
        });
    }

    /// The reply for `id`, with the moment it was computed for.
    ///
    /// A reply from a superseded moment is dropped here rather than handed out
    /// for a caller to compare — which is what stops a buddy saying "put me
    /// down" from the floor it landed on.
    pub fn take(&mut self, id: &InstanceId) -> Option<Answered> {
        let slot = self.slots.get_mut(id)?;
        while let Ok(delivered) = slot.rx.try_recv() {
            if delivered.epoch != slot.epoch {
                continue;
            }
            slot.waiting = false;
            slot.reactive = false;
            return Some(delivered.answered);
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
    model: &mut Option<Arc<ModelDirector<AnyCompleter>>>,
    behaviors: impl IntoIterator<Item = impl Into<String>>,
    character: impl Into<String>,
    settings: &DirectorSettings,
    configured: bool,
) {
    slots.abandon(id);
    *model = configured.then(|| {
        Arc::new(ModelDirector::new(
            completer_from(settings).expect("configured means a Completer exists"),
            behaviors,
            id.clone(),
            character,
        ))
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ai_buddy_core::director::Happened;

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
        with_vars(key, base, model, None, None, body)
    }

    /// Run `body` with `AI_BUDDY_DIRECTOR` exported as `value` and the other
    /// three cleared, so the developer's shell cannot decide the result.
    pub(crate) fn with_env_switch(value: &str, body: impl FnOnce()) {
        with_vars(None, None, None, Some(value), None, body)
    }

    /// The same for `AI_BUDDY_HARNESS`, which owns the Completer source rows
    /// (#436). Under this lock rather than one of its own: the settings tests
    /// read that row through the same `env_override` as the endpoint rows.
    pub(crate) fn with_harness(value: Option<&str>, body: impl FnOnce()) {
        with_vars(None, None, None, None, value, body)
    }

    fn with_vars(
        key: Option<&str>,
        base: Option<&str>,
        model: Option<&str>,
        enabled: Option<&str>,
        harness: Option<&str>,
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

        // The five the caller sets, and every Development variable: a shell
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
            (crate::harness::VAR, harness),
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
        with_vars(None, None, None, Some("on"), None, || {
            let settings = resolve("http://localhost:11434", "gemma4", None);
            let mut config = config_from(&settings);
            config.apply_switch(false);
            assert!(config.enabled, "the file said off, the process said on");
        });
    }

    /// On is still a request, not a Completer: the endpoint has to exist.
    #[test]
    fn an_exported_on_cannot_conjure_a_completer() {
        with_vars(None, None, None, Some("on"), None, || {
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
        with_vars(None, None, None, Some("banana"), None, || {
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

    /// The server that will not stream sends the same failure whole: no
    /// `content` key, and the cap named in `finish_reason` or in
    /// `incomplete_details`. Measured on oMLX with `gpt-oss-20b` (#597).
    #[test]
    fn a_whole_body_that_hit_the_cap_says_so() {
        let spent = r#"{"choices":[{"message":{"role":"assistant","reasoning_content":"hmm"},
            "finish_reason":"length"}]}"#;
        assert!(content_from_body(spent).is_err());
        assert!(truncated_body(spent));

        let incomplete = r#"{"status":"incomplete",
            "incomplete_details":{"reason":"max_output_tokens"},"output":[]}"#;
        assert!(truncated_body(incomplete));

        let done = r#"{"choices":[{"message":{"content":"stroll"},"finish_reason":"stop"}]}"#;
        assert!(!truncated_body(done), "a natural stop is not a truncation");
        assert!(!truncated_body("not json"));
    }

    fn local_endpoint() -> Endpoint {
        endpoint_at("http://localhost:11434/v1/chat/completions")
    }

    fn endpoint_at(url: &str) -> Endpoint {
        Endpoint {
            api_key: String::new(),
            url: url.to_string(),
            model: "gemma4".to_string(),
            timeout: TIMEOUT,
            max_tokens: HOSTED_MAX_TOKENS,
            session: Mutex::new(Session::default()),
            streams: AtomicBool::new(true),
            takes_effort: AtomicBool::new(true),
            agent: ureq::agent(),
        }
    }

    /// A loopback server that refuses any request naming `field`, and hands
    /// back every body it was sent.
    ///
    /// A stub rather than a product, because none of the local servers in
    /// scope rejects an unknown field: oMLX answers 200 and ignores it
    /// (`docs/research/reasoning-versus-the-final-answer.md` §6.1). The
    /// strict server the guard exists for is real — the wording here is
    /// OpenAI's — but it is not one that can be run on this machine, so the
    /// path is exercised against its contract instead.
    fn server_refusing(field: Field) -> (String, Receiver<String>) {
        use std::io::{BufRead, BufReader, Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound port").port();
        let (sent, seen) = mpsc::channel();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().expect("the same socket"));
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                if reader.read_exact(&mut body).is_err() {
                    return;
                }
                let body = String::from_utf8_lossy(&body).to_string();
                let (status, payload) = if body.contains(field.name()) {
                    (
                        "400 Bad Request",
                        format!(
                            r#"{{"error":{{"message":"Unrecognized request argument supplied: {}"}}}}"#,
                            field.name()
                        ),
                    )
                } else {
                    (
                        "200 OK",
                        r#"{"choices":[{"message":{"content":"stroll\nhey"},"finish_reason":"stop"}]}"#
                            .to_string(),
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.flush();
                if sent.send(body).is_err() {
                    return;
                }
            }
        });
        (format!("http://127.0.0.1:{port}/v1/chat/completions"), seen)
    }

    /// #612's whole contract, end to end: the field goes out, a server that
    /// names it in a rejection gets one more request without it, and the
    /// next wake does not ask again.
    #[test]
    fn a_server_that_refuses_the_effort_field_is_asked_once_and_never_again() {
        let (url, seen) = server_refusing(Field::Effort);
        let endpoint = endpoint_at(&url);
        // Whole-body, so the stub can answer in one JSON object. The stream
        // field has its own retry and #302's tests.
        endpoint.streams.store(false, Ordering::SeqCst);

        assert_eq!(
            endpoint.post(&url, "hello").unwrap().text,
            "stroll\nhey",
            "the refusal costs a second POST, not the wake"
        );
        let asked = seen.recv().expect("the first request");
        assert!(asked.contains("reasoning_effort"), "sent optimistically");
        let retried = seen.recv().expect("the retry");
        assert!(
            !retried.contains("reasoning_effort"),
            "the retry drops the field the server just named"
        );

        endpoint.post(&url, "what just happened: poked").unwrap();
        let next_wake = seen.recv().expect("the next wake");
        assert!(
            !next_wake.contains("reasoning_effort"),
            "a host that refused it once is not asked again"
        );
        assert!(
            seen.try_recv().is_err(),
            "and the second wake pays one POST, not two"
        );
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
            .close_turn(opening, Ok(Reply::whole("stroll")))
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

    /// The session is what the model reads its own last turn back from, so a
    /// turn the cap ended says so there: otherwise the next reply is written
    /// against a sentence the model appears to have simply abandoned (#610).
    ///
    /// The mark reaches the session and not the reply the Director parses.
    /// `parse_proposal` reads the first whole-Behavior-name line and speaks
    /// the rest, so a mark in the parsed text is a mark the buddy says out
    /// loud — which is the failure `bubble.js` wrote down for the bubble and
    /// which this ordering is what prevents.
    #[test]
    fn the_session_keeps_the_mark_and_the_parser_never_sees_it() {
        let endpoint = local_endpoint();

        let (opening, _) = endpoint.open_turn("what now?");
        let handed = endpoint
            .close_turn(opening, Ok(Reply::truncated("prowl")))
            .unwrap();

        assert_eq!(
            handed.text, "prowl",
            "the Director parses the model's own words, with no mark in them"
        );
        assert_eq!(
            ai_buddy_core::director::parse_proposal(&handed.text)
                .unwrap()
                .dialogue,
            None,
            "a marked text would be parsed as a Behavior with the mark as its line"
        );
        assert_eq!(
            spoken(&endpoint.session.lock().unwrap().messages),
            [
                ("user", "what now?"),
                ("assistant", "prowl\n[response truncated]")
            ],
            "the session says where the model was stopped"
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
            .close_turn(opening, Ok(Reply::whole("stroll")))
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
        endpoint.close_turn(poked, Ok(Reply::whole("nap"))).unwrap();

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

        endpoint.close_turn(poked, Ok(Reply::whole("nap"))).unwrap();
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

    /// How a whole stream ended, for the shapes whose thoughts are not what
    /// is being asserted.
    fn streamed(sse: &str) -> Streamed {
        streamed_with_thoughts(sse).0
    }

    /// The same, and every line the thought strip was told to draw.
    fn streamed_with_thoughts(sse: &str) -> (Streamed, Vec<String>) {
        let drawn = std::cell::RefCell::new(Vec::new());
        let ended = read_stream(
            std::io::Cursor::new(sse),
            || false,
            |line| drawn.borrow_mut().push(line.to_string()),
        )
        .unwrap();
        (ended, drawn.into_inner())
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
        assert_eq!(streamed(sse), Streamed::Complete("stroll\nhey".to_string()));
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
        assert_eq!(streamed(sse), Streamed::Complete("stroll\nhey".to_string()));
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
            read_stream(dribble, || false, |_| {}).unwrap(),
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
        assert_eq!(streamed(whole), Streamed::NotEventStream);

        let spent = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"hmm\"}}]}\n\n",
            "data: [DONE]\n\n",
        );
        let (ended, drawn) = streamed_with_thoughts(spent);
        assert_eq!(
            ended,
            Streamed::Complete(String::new()),
            "a model that thought its whole budget away did stream"
        );
        assert_eq!(
            drawn,
            ["hmm", ""],
            "and the thought it spent it on was drawn and then taken away"
        );
    }

    /// Which half of the wire a delta is, one shape at a time. Every field
    /// name here is one `docs/research/reasoning-versus-the-final-answer.md`
    /// found a server in scope sending (§2.2, §2.4).
    #[test]
    fn a_marked_reasoning_delta_is_a_thought_and_content_is_still_speech() {
        let read = |payload| {
            let event = read_event(payload);
            (event.thought, event.delta)
        };

        // llama.cpp, oMLX, SGLang, LM Studio for R1.
        assert_eq!(
            read(r#"{"choices":[{"delta":{"reasoning_content":"hmm"}}]}"#),
            (Some("hmm".to_string()), None)
        );
        // vLLM as of its rename, Ollama, LM Studio for gpt-oss.
        assert_eq!(
            read(r#"{"choices":[{"delta":{"reasoning":"hmm"}}]}"#),
            (Some("hmm".to_string()), None)
        );
        // Responses types its reasoning apart from its answer.
        assert_eq!(
            read(r#"{"type":"response.reasoning_summary_text.delta","delta":"hmm"}"#),
            (Some("hmm".to_string()), None)
        );
        assert_eq!(
            read(r#"{"choices":[{"delta":{"content":"stroll"}}]}"#),
            (None, Some("stroll".to_string())),
            "an unmarked delta is the reply, and nothing here second-guesses it"
        );
        assert_eq!(
            read(r#"{"type":"response.output_text.delta","delta":"stroll"}"#),
            (None, Some("stroll".to_string()))
        );
        assert_eq!(
            read(r#"{"choices":[{"delta":{"reasoning":"hmm","content":"stroll"}}]}"#),
            (Some("hmm".to_string()), Some("stroll".to_string())),
            "a server that marks both in one frame is read for both"
        );
    }

    /// The whole point: a reasoning model's thinking reaches the strip and
    /// never the reply, so nothing the buddy says out loud was thought at it
    /// (ADR-0025). The strip draws the line being written now, not the chunk
    /// it arrived in, and the end of the turn takes it away.
    #[test]
    fn thinking_is_drawn_while_a_turn_runs_and_never_joins_the_reply() {
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"the user\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\" waved\\nso\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\" wave back\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"wave\\nhey\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(
            streamed_with_thoughts(sse),
            (
                Streamed::Complete("wave\nhey".to_string()),
                ["the user", "so", "so wave back", ""]
                    .map(str::to_string)
                    .to_vec()
            )
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
            streamed(responses),
            Streamed::Complete("stroll".to_string())
        );

        let completions = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        );
        assert_eq!(
            streamed(completions),
            Streamed::Complete("stroll".to_string())
        );

        let cut = "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\\nhey th\"}}]}\n\n";
        assert_eq!(streamed(cut), Streamed::Cut);
    }

    /// A model that spends its whole budget thinking ends with
    /// `finish_reason: "length"` and no content — measured on `gpt-oss-20b`
    /// at 512 tokens on about 40% of wakes (#597). That is not a reply, and
    /// the value of the field is the only thing that says so: `stop` and
    /// `length` are both strings.
    #[test]
    fn an_empty_length_finish_is_a_truncation_and_a_stop_finish_is_a_reply() {
        let spent = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"hmm\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(
            streamed(spent),
            Streamed::Truncated(String::new()),
            "the budget ran out before any text, which is not an empty reply"
        );

        let done = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(
            streamed(done),
            Streamed::Complete("stroll".to_string()),
            "a natural stop is untouched by reading the value"
        );
    }

    /// A cap reached after the model started writing is a truncation too, and
    /// is refused for the reason #302 refuses a cut stream: what arrived is
    /// half a sentence. The text is kept only so the log can say the budget
    /// ran out writing rather than thinking.
    #[test]
    fn a_length_finish_that_wrote_text_is_a_truncation_too() {
        let clipped = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"stroll\\nhey th\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        assert_eq!(
            streamed(clipped),
            Streamed::Truncated("stroll\nhey th".to_string())
        );
    }

    /// Best effort, because we are the ones who cut the model off: what it
    /// wrote is handed on, marked. With nothing written there is nothing to
    /// show, and the Action Log line names the cap and the knob either way.
    #[test]
    fn a_truncation_shows_what_it_wrote_and_silence_when_it_wrote_nothing() {
        let endpoint = Endpoint {
            max_tokens: LOCAL_MAX_TOKENS,
            ..local_endpoint()
        };
        let url = "http://127.0.0.1:1234/v1/chat/completions";

        let thinking = endpoint.reply_from(url, Streamed::Truncated(String::new()));
        let Err(Unsent::Truncated(why)) = thinking else {
            panic!("a budget spent entirely on thinking has nothing to show");
        };
        assert!(
            why.contains("spent all 512 tokens thinking") && why.contains(MAX_TOKENS),
            "the log line names the cap and the knob: {why}"
        );

        assert_eq!(
            endpoint
                .reply_from(url, Streamed::Truncated("stroll\nhey th".to_string()))
                .ok(),
            Some(Reply::truncated("stroll\nhey th")),
            "the Behavior is acted on and the words are said, with the mark beside them"
        );

        assert_eq!(
            endpoint
                .reply_from(url, Streamed::Complete("stroll".to_string()))
                .ok(),
            Some(Reply::whole("stroll")),
            "a whole reply is not marked"
        );

        assert!(
            endpoint
                .out_of_budget(url, false)
                .contains("cut off mid-reply"),
            "the log tells a budget spent thinking from a line that ran out"
        );
    }

    /// Responses ends a truncated reply with `response.incomplete` rather
    /// than `response.completed`, and sends no `[DONE]` after it. With no arm
    /// for that event the body ended unmarked, so a truncation read as a cut
    /// connection and was re-asked whole.
    #[test]
    fn a_responses_incomplete_is_a_truncation_not_a_cut() {
        let sse = concat!(
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"hmm\"}\n\n",
            "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\",\
             \"incomplete_details\":{\"reason\":\"max_output_tokens\"}}}\n\n",
        );
        assert_eq!(streamed(sse), Streamed::Truncated(String::new()));
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
            read_stream(Endless, abandoned, |_| {}).unwrap(),
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
            false,
        );
        assert_eq!(streamed["stream"], true);
        let responses = request_body(
            "grok-4.6",
            &session,
            true,
            HOSTED_MAX_TOKENS,
            Wire::Stream,
            false,
        );
        assert_eq!(responses["stream"], true, "the Responses path streams too");

        let whole = request_body(
            "gpt-4o-mini",
            &session,
            false,
            HOSTED_MAX_TOKENS,
            Wire::Whole,
            false,
        );
        assert!(
            whole.get("stream").is_none(),
            "a retry must not name the field the server just refused"
        );
    }

    /// #612: the field is the one lever measured to change the empty-reply
    /// rate, and the retry is only honest if the second body drops it.
    #[test]
    fn a_chat_request_asks_for_low_effort_and_the_fallback_does_not() {
        let session = [Message {
            role: "user",
            content: "wave".to_string(),
        }];
        let asked = request_body(
            "gpt-oss-20b",
            &session,
            false,
            LOCAL_MAX_TOKENS,
            Wire::Stream,
            true,
        );
        assert_eq!(asked["reasoning_effort"], "low");

        let dropped = request_body(
            "gpt-oss-20b",
            &session,
            false,
            LOCAL_MAX_TOKENS,
            Wire::Stream,
            false,
        );
        assert!(
            dropped.get("reasoning_effort").is_none(),
            "a retry must not name the field the server just refused"
        );

        let responses = request_body(
            "grok-4.6",
            &session,
            true,
            HOSTED_MAX_TOKENS,
            Wire::Whole,
            true,
        );
        assert!(
            responses.get("reasoning_effort").is_none(),
            "the Responses path spells its effort under `reasoning`"
        );
        assert_eq!(responses["reasoning"]["effort"], "low");
    }

    #[test]
    fn a_responses_request_uses_input_and_does_not_store() {
        let session = [Message {
            role: "user",
            content: "wave".to_string(),
        }];
        let body = request_body(
            "grok-4.6",
            &session,
            true,
            HOSTED_MAX_TOKENS,
            Wire::Whole,
            false,
        );
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
        let body = request_body(
            "grok-4.6",
            &session,
            true,
            HOSTED_MAX_TOKENS,
            Wire::Whole,
            false,
        );
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
        assert_eq!(
            refused_field(
                400,
                r#"{"error":{"message":"Unrecognized request argument supplied: stream"}}"#
            ),
            Some(Field::Stream)
        );
        assert_eq!(
            refused_field(
                422,
                r#"{"detail":[{"loc":["body","stream"],"msg":"extra fields not permitted"}]}"#
            ),
            Some(Field::Stream),
            "a strict server validates the body rather than the field"
        );
        assert_eq!(
            refused_field(400, r#"{"error":{"message":"model gpt-9 does not exist"}}"#),
            None,
            "a 400 about anything else would fail the same way twice"
        );
        assert_eq!(
            refused_field(403, "streaming is not available"),
            None,
            "403 is the key, the credits, or a path ACL; fallback_url owns that"
        );
        assert_eq!(
            refused_field(400, "upstream connect error or disconnect/reset"),
            None,
            "a gateway saying upstream is not a server naming the stream field"
        );
        assert_eq!(
            refused_field(400, r#"{"error":"streaming is not supported here"}"#),
            Some(Field::Stream),
            "the word can still be inflected, it just cannot be a suffix"
        );
    }

    /// #612: the same read as the stream field, on the field #597 measured.
    /// A body that names it is the only thing that drops it, because the
    /// cost of reading a plain 400 as a refusal is a second POST that fails
    /// the same way.
    #[test]
    fn a_server_that_rejects_the_effort_field_earns_one_retry_without_it() {
        assert_eq!(
            refused_field(
                400,
                r#"{"error":{"message":"Unrecognized request argument supplied: reasoning_effort"}}"#
            ),
            Some(Field::Effort)
        );
        assert_eq!(
            refused_field(
                400,
                r#"{"error":{"message":"Unsupported parameter: 'reasoning_effort' is not supported with this model."}}"#
            ),
            Some(Field::Effort),
            "OpenAI's wording for a field a non-reasoning model will not take"
        );
        assert_eq!(
            refused_field(
                422,
                r#"{"detail":[{"loc":["body","reasoning_effort"],"msg":"extra fields not permitted"}]}"#
            ),
            Some(Field::Effort)
        );
        assert_eq!(
            refused_field(400, r#"{"error":{"message":"model gpt-9 does not exist"}}"#),
            None,
            "a 400 about anything else would fail the same way twice"
        );
        assert_eq!(
            refused_field(403, "reasoning_effort is not available"),
            None,
            "403 is the key, the credits, or a path ACL; fallback_url owns that"
        );
        assert_eq!(
            refused_field(
                400,
                r#"{"error":{"message":"unknown fields: reasoning_effort, stream"}}"#
            ),
            Some(Field::Effort),
            "a validator listing both drops the field this request added first"
        );
    }

    #[test]
    fn a_broken_stream_is_worth_a_retry_but_teaches_nothing() {
        assert_eq!(
            Unsent::Refused(Field::Stream, "names the field".to_string()).retry_settles(),
            Some((Field::Stream, true)),
            "a host that rejected the field will reject it on the next wake too"
        );
        assert_eq!(
            Unsent::Refused(Field::Effort, "names the field".to_string()).retry_settles(),
            Some((Field::Effort, true)),
            "and the effort field is remembered the same way"
        );
        assert_eq!(
            Unsent::Cut("ended mid-reply".to_string()).retry_settles(),
            Some((Field::Stream, false)),
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
        assert_eq!(
            Unsent::Truncated("512 tokens".to_string()).retry_settles(),
            None,
            "the same question at the same cap gets the same nothing"
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

    /// The suffix test is the half that matters for a lookalike name, and the
    /// normalised host is the half the old string cut got wrong: it compared
    /// the port and the spelling along with the name, so an endpoint written
    /// with `:443` or in capitals was not xAI and took the legacy path.
    ///
    /// The lookalike cases pass either way and are pinned as regression
    /// guards, not as repairs.
    #[test]
    fn only_xais_own_hosts_answer_to_its_inference_path() {
        assert!(host_is_xai("https://api.x.ai/v1/responses"));
        assert!(host_is_xai("https://mtls.api.x.ai"), "a real subdomain");
        assert!(
            host_is_xai("https://api.x.ai:443/v1"),
            "an explicit port is not part of the name"
        );
        assert!(
            host_is_xai("https://API.X.AI/v1"),
            "nor is how the row was capitalised"
        );
        assert!(
            !host_is_xai("https://evil-api.x.ai"),
            "the dot is what makes it a subdomain"
        );
        assert!(
            !host_is_xai("https://api.x.ai.evil.com"),
            "a name that opens with theirs"
        );
        assert!(
            !host_is_xai("https://evil.com/api.x.ai"),
            "theirs in the path, not the host"
        );
        assert!(!host_is_xai("api.x.ai"), "no scheme, no host");
    }

    /// The probe hangs `/v1/models` off this and prints it. Production change
    /// that would fail this: cutting at the first `/` after the scheme, which
    /// keeps userinfo — and puts a password in a trace line.
    #[test]
    fn an_origin_keeps_the_port_and_drops_the_credentials() {
        assert_eq!(
            origin("http://localhost:11434/v1"),
            "http://localhost:11434"
        );
        assert_eq!(
            origin("https://api.openai.com/v1"),
            "https://api.openai.com"
        );
        assert_eq!(
            origin("https://user:sk-secret@api.x.ai/v1/responses"),
            "https://api.x.ai"
        );
        assert_eq!(
            origin("https://api.openai.com:443/v1"),
            "https://api.openai.com",
            "the default port is not part of the origin"
        );
        assert_eq!(
            origin("localhost:11434"),
            "localhost:11434",
            "no host to serialize, so the value is handed back for the probe \
             to fail on and name"
        );
    }

    /// A query that mentions the path is not the path.
    #[test]
    fn the_responses_path_is_the_path_and_not_the_query() {
        assert!(uses_responses("https://api.x.ai/v1/responses"));
        assert!(!uses_responses("https://api.x.ai/v1/chat/completions"));
        assert!(!uses_responses(
            "https://api.x.ai/v1/chat/completions?from=/responses"
        ));
    }

    /// What the Chat header is handed. The userinfo case is the same one
    /// `is_local` turns on, and it is drawn as well as decided on, so a
    /// password written into the row must not reach the window (#474).
    #[test]
    fn a_host_is_named_without_its_credentials_or_its_path() {
        assert_eq!(host_of("https://api.openai.com/v1"), "api.openai.com");
        assert_eq!(host_of("http://localhost:8000"), "localhost:8000");
        assert_eq!(host_of("http://[fd00::1]:8080"), "[fd00::1]:8080");
        assert_eq!(
            host_of("https://user:sk-secret@api.openai.com/v1"),
            "api.openai.com",
            "neither half of the userinfo is drawn"
        );
        assert_eq!(
            host_of("http://10.0.0.1@172.16.evil.com/"),
            "172.16.evil.com",
            "the digits are userinfo; the host is evil.com"
        );
    }

    /// A base with no scheme is not a URL anything can be posted to —
    /// `completions_url` concatenates onto it — so it names no host and is not
    /// local. Remote is the safe half of that: it keeps the key required.
    #[test]
    fn a_base_with_no_scheme_names_no_host_and_is_not_local() {
        assert_eq!(host_of("localhost:8000"), "");
        assert_eq!(host_of(""), "");
        assert!(!is_local("localhost:8000"));
        assert!(!is_local("api.openai.com"));
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

    /// The read site, where `dev_flags` only holds the decision: a wait no
    /// source names is `Pace::FIRST`, not zero seconds.
    #[test]
    fn an_exported_wake_interval_outranks_the_persisted_one() {
        with_env(None, None, None, || {
            let file = crate::settings::Settings {
                director_wake_secs: "300".into(),
                ..Default::default()
            };
            crate::dev_flags::seed(&file);
            assert_eq!(ambient_first(), Duration::from_secs(300));

            std::env::set_var(WAKE_SECS, "30");
            crate::dev_flags::seed(&file);
            assert_eq!(ambient_first(), Duration::from_secs(30));
            std::env::remove_var(WAKE_SECS);

            crate::dev_flags::seed(&crate::settings::Settings::default());
            assert_eq!(ambient_first(), Pace::FIRST);
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
            instance_prompt: String::new(),
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
                    id.clone(),
                    "cat",
                )),
                wake_context(),
            );
            let mut model = None;

            retarget_model(
                &mut slots,
                &id,
                &mut model,
                ["stroll"],
                "cat",
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
                "cat",
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
                "cat",
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
        fn complete(&self, _: &WakeRequest) -> Result<Reply, String> {
            for _ in 0..400 {
                if abandoned() {
                    self.saw.store(true, Ordering::SeqCst);
                    return Err("abandoned".to_string());
                }
                thread::sleep(Duration::from_millis(5));
            }
            Ok(Reply::whole("idle"))
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
        fn complete(&self, _: &WakeRequest) -> Result<Reply, String> {
            thread::sleep(self.delay);
            Ok(Reply::whole(self.behavior))
        }
    }

    fn answering(behavior: &'static str, delay_ms: u64) -> Arc<ModelDirector<Answers>> {
        Arc::new(ModelDirector::new(
            Answers {
                behavior,
                delay: Duration::from_millis(delay_ms),
            },
            ["stroll", "nap"],
            "buddy",
            "cat",
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
    fn polled(slots: &mut Slots, id: &InstanceId) -> Option<Answered> {
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

        let answered = polled(&mut slots, &id).expect("the newest call answers");
        assert_eq!(behavior_of(&answered.wake), "nap");

        thread::sleep(Duration::from_millis(200));
        assert!(
            slots.take(&id).is_none(),
            "the superseded reply must be dropped, not delivered a tick later"
        );
    }

    /// #243, #435: a name nobody declared arrives as speech, so the name is the
    /// only thing that tells the two apart — and the Action Log is written at
    /// `take`, not in the worker, so it has to survive the trip.
    #[test]
    fn take_carries_the_near_miss_the_worker_saw() {
        let mut slots = Slots::new();
        let id = "buddy".to_string();

        // `answering` declares stroll and nap, so cartwheel is neither.
        slots.wake(&id, answering("cartwheel", 0), wake_context());

        let answered = polled(&mut slots, &id).expect("the call answers");
        assert_eq!(answered.near_miss.as_deref(), Some("cartwheel"));
        assert_eq!(
            behavior_of(&answered.wake),
            "",
            "a near miss is still played as speech"
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

        let carried = polled(&mut slots, &id).expect("the call answers").context;
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

        let theirs = polled(&mut slots, &second).expect("the second buddy still answers");
        assert_eq!(behavior_of(&theirs.wake), "nap");
        let ours = polled(&mut slots, &first).expect("the first buddy answers too");
        assert_eq!(behavior_of(&ours.wake), "nap");
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
                id.clone(),
                "cat",
            )),
            wake_context(),
        );
        slots.wake(&id, answering("nap", 0), wake_context());

        assert!(
            waited_for(&saw),
            "the superseded worker ran on without ever seeing that it had been dropped"
        );
    }

    /// How #244's prompt phrasings differ.
    ///
    /// The personality file itself is never touched: the sample lines are why
    /// the voices read as well as they do (#156), so only the frame around
    /// them moves.
    #[derive(Clone, Copy, Debug)]
    enum Framing {
        /// The prompt as shipped — personality first, format instruction after.
        Today,
        /// The quoted lines named as voice rather than as a reply to imitate.
        Framed,
        /// The personality moved below the format instruction.
        After,
    }

    /// The last line of the format instruction, and the seam `After` cuts on.
    const FORMAT_ENDS: &str = "Propose nothing else.\n";

    const VOICE_NOTE: &str = "Those quoted lines are how this character sounds, \
        not a format to copy: your own reply still begins with a behavior name.";

    /// `prompt` said under `framing`.
    ///
    /// A rewrite of the built prompt rather than a second prompt builder, so
    /// the harness cannot drift from the one production sends. A later turn
    /// carries no Personality Prompt and comes back untouched.
    fn reframed(prompt: &str, personality: &str, framing: Framing) -> String {
        // An empty Personality Prompt renders as "(no personality)", which
        // `strip_prefix("")` would happily match and then frame as a voice.
        if personality.is_empty() {
            return prompt.to_string();
        }
        let Some(rest) = prompt.strip_prefix(personality) else {
            return prompt.to_string();
        };
        match framing {
            Framing::Today => prompt.to_string(),
            Framing::Framed => format!("{personality}\n\n{VOICE_NOTE}{rest}"),
            Framing::After => match rest.split_once(FORMAT_ENDS) {
                Some((head, tail)) => {
                    format!("{}{FORMAT_ENDS}\n{personality}\n{tail}", head.trim_start())
                }
                None => prompt.to_string(),
            },
        }
    }

    /// The quoted sample lines a Personality Prompt offers, per #156's
    /// convention ("It has been heard to say: …").
    ///
    /// ponytail: quote-character parity rather than the "heard to say" anchor,
    /// which black-mage already words differently. It costs nothing and holds
    /// for all eight shipped personalities; one unpaired quote in a future one
    /// would invert it and the harness would report a clean zero. Anchor on
    /// the colon if a personality ever needs a lone quote character.
    fn sample_lines(personality: &str) -> Vec<&str> {
        personality
            .split(['"', '\u{201c}', '\u{201d}'])
            .skip(1)
            .step_by(2)
            .collect()
    }

    /// Below this many squashed characters a quoted string is too short to
    /// be a sample line: `"Mad Cat"` appears inside timber-wolf's prose, and
    /// matching it would count any reply that used the name.
    const QUOTE_FLOOR: usize = 12;

    /// The sample line `reply` said back, when it said one back.
    ///
    /// ponytail: the reply containing a whole sample line, compared over
    /// squashed text, rather than an edit distance. It catches the line
    /// repunctuated, recased, or wrapped in a preamble, which is what #230
    /// saw; a model that truncates or paraphrases it reads as prose and is
    /// not counted. So the number is a floor on quoting, never an inflated
    /// one — reach for a similarity measure only if the paraphrases matter.
    fn quoted_sample_line(reply: &str, personality: &str) -> Option<String> {
        let said = squashed(reply);
        sample_lines(personality)
            .into_iter()
            .find(|line| {
                let sample = squashed(line);
                sample.len() >= QUOTE_FLOOR && said.contains(&sample)
            })
            .map(str::to_string)
    }

    /// `text` as its lowercase alphanumeric words, single-spaced.
    fn squashed(text: &str) -> String {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Sends every wake under one phrasing.
    ///
    /// A Completer decorator, so `ModelDirector` still builds the prompt and
    /// still classifies the reply — the comparison changes the wording and
    /// nothing else.
    struct Reframing<C> {
        inner: C,
        personality: String,
        framing: Framing,
    }

    impl<C: Completer> Completer for Reframing<C> {
        fn complete(&self, request: &WakeRequest) -> Result<Reply, String> {
            let mut sent = request.clone();
            sent.prompt = reframed(&sent.prompt, &self.personality, self.framing);
            self.inner.complete(&sent)
        }
    }

    /// A Completer that fails every turn, for a test that only needs the
    /// prompt `ModelDirector` builds.
    struct Silent;

    impl Completer for Silent {
        fn complete(&self, _request: &WakeRequest) -> Result<Reply, String> {
            Err("no server here".to_string())
        }
    }

    /// A personality in #156's shape, small enough to reason about.
    const TWO_SAMPLES: &str = "Cat claimed the desktop.\n\nIt has been heard to say: \
        \"What is that one? Show me.\" - \"You may continue.\"";

    /// #244: the quote counter has to recognise a sample line said back with
    /// the model's own punctuation, and refuse a fragment short enough to
    /// turn up in any sentence.
    #[test]
    fn a_sample_line_said_back_is_recognised_however_it_is_punctuated() {
        let personality = TWO_SAMPLES;

        assert_eq!(
            quoted_sample_line("What is that one? Show me.", personality).as_deref(),
            Some("What is that one? Show me.")
        );
        assert_eq!(
            quoted_sample_line("what is that one - show me!!", personality).as_deref(),
            Some("What is that one? Show me."),
            "the punctuation is the model's, not the personality's"
        );
        assert_eq!(
            quoted_sample_line("Hmm. What is that one? Show me.", personality).as_deref(),
            Some("What is that one? Show me."),
            "a sample line with a preamble is still a quote"
        );
    }

    #[test]
    fn prose_of_its_own_is_not_counted_as_a_quote() {
        let personality = TWO_SAMPLES;

        assert_eq!(
            quoted_sample_line("I supervised that compile and I approve.", personality),
            None
        );
        assert_eq!(
            quoted_sample_line("Show me.", personality),
            None,
            "part of a sample line is the model's own sentence, not a quote"
        );
        assert_eq!(
            quoted_sample_line(
                "What is that one? Show me.",
                "A personality with no samples."
            ),
            None
        );
        assert_eq!(
            quoted_sample_line(
                "It is a \"Mad Cat\" and I caught it.",
                "It is a \"Mad Cat\"."
            ),
            None,
            "a quoted string too short to be a sample line is a coincidence"
        );
    }

    /// Against the shipped file, not a fixture. The convention separates the
    /// quoted lines with em dashes and wraps them mid-sentence, and extraction
    /// that failed on that would report a clean zero rather than an error.
    #[test]
    fn every_sample_line_the_shipped_cat_offers_is_recognised_verbatim() {
        let personality = include_str!("../../characters/cat/personality.txt");
        let lines = sample_lines(personality);

        assert!(
            !lines.is_empty(),
            "the cat still offers sample lines (#156)"
        );
        for line in lines {
            assert_eq!(
                quoted_sample_line(line, personality).as_deref(),
                Some(line),
                "a shipped sample line said back verbatim has to count"
            );
        }
    }

    #[test]
    fn framing_moves_the_personality_and_leaves_the_rest_alone() {
        let personality =
            "Cat claimed the desktop. It has been heard to say: \"Show me that one.\"";
        let director = ModelDirector::new(Silent, ["stroll", "nap"], "buddy", "Cat");
        let today = director.prompt(&Context {
            personality: personality.to_string(),
            happened: Happened::Poke,
            standing: "the display floor".to_string(),
            ..wake_context()
        });

        assert_eq!(reframed(&today, personality, Framing::Today), today);

        let framed = reframed(&today, personality, Framing::Framed);
        assert!(framed.starts_with(personality), "the voice still opens");
        assert!(
            framed.contains("not a format to copy"),
            "the sample lines are marked as voice: {framed}"
        );

        let after = reframed(&today, personality, Framing::After);
        assert!(!after.starts_with(personality), "the voice no longer opens");
        assert!(
            after.find(FORMAT_ENDS) < after.find(personality),
            "and it now follows the format instruction: {after}"
        );
    }

    #[test]
    fn a_later_turn_carries_no_personality_and_is_left_alone() {
        let follow_up = "what just happened: poked\nrecent: (none)\n";
        for framing in [Framing::Today, Framing::Framed, Framing::After] {
            assert_eq!(
                reframed(follow_up, "Cat claimed the desktop.", framing),
                follow_up,
                "{framing:?} rewrote a turn that carries no personality"
            );
        }
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
    /// `AI_BUDDY_BENCH_FRAMING` picks #244's phrasing — `today` (the default),
    /// `framed`, or `after` — and the run reports how much of its prose was a
    /// personality sample line quoted back.
    ///
    /// #244 was answered here and the wording left alone: over 1800 wakes of
    /// `gemma-4-e2b-it-4bit` (three framings, three runs of 200 each) only 4
    /// of 118 prose replies quoted a sample line, and the contract-break rate
    /// wandered 0–20% *between runs of the same framing* — a wider spread
    /// than any gap between the framings. Quoting is real and rare; #230's
    /// two-in-four was a small sample. The breaks are invented names, so
    /// #144's schema is still the thing that would fix them.
    ///
    /// A second model reached the same verdict by a different road. Over
    /// 1200 wakes of `gpt-oss-20b-MXFP4-Q8` (three framings, two runs of 200
    /// each) no reply quoted a sample line at all, and the widest spread
    /// between runs of one framing — 9pp — still covers the 8pp span between
    /// the framings' means, so again no wording can be called better. But
    /// its breaks are not choices about wording: it is a reasoning model, and
    /// on 40% of wakes the thinking trace spends `LOCAL_MAX_TOKENS` before
    /// any content is written, so the reply arrives empty. No phrasing of the
    /// personality reaches a token budget. Read it as widening the "sample
    /// lines are not the cause" finding to two models, and as leaving
    /// invented names measured on `gemma-4-e2b-it-4bit` alone — gpt-oss
    /// rarely got far enough to invent one.
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

        let framing = match std::env::var("AI_BUDDY_BENCH_FRAMING")
            .unwrap_or_default()
            .as_str()
        {
            "framed" => Framing::Framed,
            "after" => Framing::After,
            _ => Framing::Today,
        };
        let director = ModelDirector::new(
            Reframing {
                inner: endpoint,
                personality: cat.personality.clone(),
                framing,
            },
            behaviors.clone(),
            "buddy",
            cat.name.clone(),
        );

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
        // #244: prose that is a sample line handed back. A subset of `speech`,
        // because a reply that names a Behavior kept the contract whatever its
        // dialogue borrowed.
        let mut quoted = 0usize;
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
                instance_prompt: String::new(),
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
                    let quote = quoted_sample_line(&said, &cat.personality);
                    if near {
                        case_only += 1;
                    } else {
                        speech += 1;
                        if quote.is_some() {
                            quoted += 1;
                        }
                    }
                    if examples.len() < 6 {
                        let tag = match (near, quote.is_some()) {
                            (true, _) => "case-only",
                            (false, true) => "quoted",
                            (false, false) => "speech",
                        };
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
        println!("  framing:   {framing:?}  (#244)");
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
        println!(
            "  quoted:     {quoted:>3}  ({:.0}%)  of it a personality sample line",
            percent(quoted)
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

    /// A directory of our own under the system temp dir, removed when the test ends.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "http-session-{label}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("temp dir is creatable");
            Self(dir)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Plan-seam change that would fail this: `note_http_call` leaving
    /// WakeRequest unread because "the Action Log belongs to the Harness".
    #[test]
    fn an_http_call_writes_prompt_and_turn() {
        let dir = TempDir::new("http-session");
        let request = WakeRequest {
            prompt: "hi".into(),
            instance: "buddy-1".into(),
            character: "bmo".into(),
            reactive: true,
        };
        note_http_call(
            dir.path(),
            &request,
            Ok(&Reply::whole("the desktop floor")),
            "unused here",
        );

        let body = std::fs::read_to_string(dir.path().join(crate::action_log::FILE)).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2, "{body}");
        let prompt: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(prompt["event"], "prompt");
        assert_eq!(prompt["instance"], "buddy-1");
        assert_eq!(prompt["wake"], "reactive");
        assert_eq!(prompt["chars"], 2);
        let turn: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(turn["event"], "turn");
        assert_eq!(turn["text"], "the desktop floor");
    }

    /// A turn the cap ended is both: words that were shown, and a reason
    /// there were no more of them. The line carries the pair, so the reader
    /// asking why the buddy stopped mid-sentence is told the cap and the
    /// model rather than reading a reply that just ends (#610).
    #[test]
    fn a_truncated_http_call_writes_the_words_and_the_cap() {
        let dir = TempDir::new("http-session-cut");
        let request = WakeRequest {
            prompt: "hi".into(),
            instance: "buddy-1".into(),
            character: "bmo".into(),
            reactive: true,
        };
        let endpoint = Endpoint {
            max_tokens: LOCAL_MAX_TOKENS,
            ..local_endpoint()
        };

        note_http_call(
            dir.path(),
            &request,
            Ok(&Reply::truncated("prowl\nMine now, and the")),
            &endpoint.out_of_budget(&endpoint.url, false),
        );

        let body = std::fs::read_to_string(dir.path().join(crate::action_log::FILE)).unwrap();
        let turn: serde_json::Value = serde_json::from_str(body.lines().nth(1).unwrap()).unwrap();
        assert_eq!(turn["text"], "prowl\nMine now, and the");
        let why = turn["truncated"].as_str().unwrap_or_default();
        assert!(
            why.contains("gemma4") && why.contains("512") && why.contains(MAX_TOKENS),
            "the line names the model, the cap and the knob: {why}"
        );
    }

    /// Plan-seam change that would fail this: `note_http_call` writing no
    /// turn line on a failed HTTP wake, so a later parsed/failed cannot be joined.
    #[test]
    fn a_failed_http_call_writes_the_error() {
        let dir = TempDir::new("http-session-err");
        let request = WakeRequest {
            prompt: "hi".into(),
            instance: "buddy-1".into(),
            character: "bmo".into(),
            reactive: false,
        };
        note_http_call(
            dir.path(),
            &request,
            Err("connection refused"),
            "unused here",
        );

        let body = std::fs::read_to_string(dir.path().join(crate::action_log::FILE)).unwrap();
        let turn: serde_json::Value = serde_json::from_str(body.lines().nth(1).unwrap()).unwrap();
        assert_eq!(turn["wake"], serde_json::Value::Null); // wake lives on prompt, not turn
        assert_eq!(turn["error"], "connection refused");
        let prompt: serde_json::Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
        assert_eq!(prompt["wake"], "proactive");
    }
}
