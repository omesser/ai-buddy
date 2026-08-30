//! Completer stand-in, env config, and the in-flight session call.
//!
//! ponytail: HTTP chat-completions until #16 attaches a Harness. The
//! `Completer` trait is the seam; this file is the disposable impl. ADR-0008.
//!
//! The Completer runs on a worker thread. The frame loop only `try_recv`s.
//! #18 binds these settings. Until then they come from the env.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ai_buddy_core::director::{Completer, Context, ModelDirector, Pace, Wake, WAKE_EVERY};
use serde::Serialize;

/// Completer timeout. After this, fall back to `StaticDirector`.
///
/// Longer than a snappy chat-completions hop: xAI's Responses path can
/// think, and 8s was enough to lose a Grok wake to Static.
pub const TIMEOUT: Duration = Duration::from_secs(20);

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

/// Read Director config from the env. No API key means `StaticDirector` only.
pub fn config() -> DirectorConfig {
    let configured = std::env::var(API_KEY)
        .ok()
        .is_some_and(|key| !key.is_empty());
    let enabled = configured && !off();
    DirectorConfig {
        enabled,
        configured,
        wake_every: WAKE_EVERY,
        ambient_first: wake_secs().unwrap_or(Pace::FIRST),
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
    let api_key = std::env::var(API_KEY).ok().filter(|key| !key.is_empty())?;
    let base = std::env::var(BASE_URL).unwrap_or_else(|_| DEFAULT_BASE.to_string());
    let model = std::env::var(MODEL).unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    Some(Endpoint {
        api_key,
        url: completions_url(&base),
        model,
        timeout: TIMEOUT,
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

pub struct Endpoint {
    api_key: String,
    url: String,
    model: String,
    timeout: Duration,
}

impl Completer for Endpoint {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        let body = request_body(&self.model, prompt, uses_responses(&self.url));
        let mut request = ureq::post(&self.url)
            .set("Content-Type", "application/json")
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
        let response = request.send_json(body).map_err(|error| error.to_string())?;
        let text = response.into_string().map_err(|error| error.to_string())?;
        content_from_body(&text)
    }
}

fn request_body(model: &str, prompt: &str, responses: bool) -> serde_json::Value {
    if responses {
        serde_json::json!({
            "model": model,
            "input": [{"role": "user", "content": prompt}],
            "max_output_tokens": 80,
            "store": false,
        })
    } else {
        serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
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
        let body = request_body("grok-4.6", "wave", true);
        assert_eq!(body["input"][0]["content"], "wave");
        assert_eq!(body["max_output_tokens"], 80);
        assert_eq!(body["store"], false);
        assert!(body.get("messages").is_none());
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
    }
}
