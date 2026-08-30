//! The model-backed Director's I/O: HTTP, env knobs, and the mailbox.
//!
//! `docs/SPEC.md` keeps the Director off the render path. A wake therefore
//! lives on its own thread, and the frame loop only `try_recv`s. Settings
//! (#18) will bind the knobs and show `DirectorInspect.last_payload`; until
//! then they are env vars and the last Character Prompt is held in memory.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ai_buddy_core::director::{
    Completer, Context, ModelDirector, Wake, MODEL_WAKE_EVERY, WAKE_EVERY,
};
use serde::Serialize;

/// How long a wake may take before the Static Director takes it instead.
///
/// Long enough for a small completion; short enough that a hung endpoint
/// does not leave the buddy without a life for a noticeable stretch.
pub const TIMEOUT: Duration = Duration::from_secs(8);

const API_KEY: &str = "AI_BUDDY_DIRECTOR_API_KEY";
const BASE_URL: &str = "AI_BUDDY_DIRECTOR_BASE_URL";
const MODEL: &str = "AI_BUDDY_DIRECTOR_MODEL";
const ENABLED: &str = "AI_BUDDY_DIRECTOR";
const WAKE_SECS: &str = "AI_BUDDY_DIRECTOR_WAKE_SECS";

const DEFAULT_BASE: &str = "https://api.openai.com";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// What the user can turn and what settings will show.
#[derive(Clone, Debug, Serialize)]
pub struct DirectorInspect {
    pub enabled: bool,
    pub configured: bool,
    pub wake_secs: u64,
    pub last_payload: Option<String>,
}

/// The knobs the frame loop honours. Read once at start; #18 will write them.
#[derive(Clone, Debug)]
pub struct Knobs {
    pub enabled: bool,
    pub configured: bool,
    pub wake_every: Duration,
}

impl Knobs {
    pub fn inspect(&self) -> DirectorInspect {
        DirectorInspect {
            enabled: self.enabled,
            configured: self.configured,
            wake_secs: self.wake_every.as_secs(),
            last_payload: None,
        }
    }
}

/// Read the env. No key means the Static Director is the whole life.
pub fn knobs() -> Knobs {
    let configured = std::env::var(API_KEY)
        .ok()
        .is_some_and(|key| !key.is_empty());
    let enabled = configured && !off();
    let wake_every = if enabled {
        wake_secs().unwrap_or(MODEL_WAKE_EVERY)
    } else {
        WAKE_EVERY
    };
    Knobs {
        enabled,
        configured,
        wake_every,
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

/// Join a provider base onto the chat-completions path without doubling `/v1`.
fn completions_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

pub struct Endpoint {
    api_key: String,
    url: String,
    model: String,
    timeout: Duration,
}

impl Completer for Endpoint {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 80,
        });
        let response = ureq::post(&self.url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .timeout(self.timeout)
            .send_json(body)
            .map_err(|error| error.to_string())?;
        let text = response.into_string().map_err(|error| error.to_string())?;
        content_from_chat(&text)
    }
}

fn content_from_chat(body: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| error.to_string())?;
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "chat completion had no message content".to_string())
}

/// A wake in flight. The frame loop starts one and `try_take`s later.
pub struct Mail {
    tx: Sender<Wake>,
    rx: Receiver<Wake>,
    busy: Arc<AtomicBool>,
}

impl Mail {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn idle(&self) -> bool {
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
            // Always send. A panic here would otherwise leave `busy` set and
            // the Static Director never asked again.
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
        assert_eq!(content_from_chat(body).unwrap(), "stroll\nhey");
    }

    #[test]
    fn a_body_without_content_is_an_error() {
        assert!(content_from_chat("{}").is_err());
        assert!(content_from_chat("not json").is_err());
    }

    #[test]
    fn the_completions_url_joins_a_base_without_doubling_v1() {
        assert_eq!(
            completions_url("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            completions_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            completions_url("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
    }
}
