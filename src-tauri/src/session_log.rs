//! Current-session Chat turns, held because `emit_to` only reaches windows that
//! exist. Permission asks already wait on `PendingAsks`; Speech of this
//! Completer session belongs in the same log (ADR-0018) even when Chat was
//! never opened.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::SystemTime;

use tauri::{Emitter, Manager};

#[derive(Clone)]
pub struct Turn {
    pub you: bool,
    pub said: Option<String>,
    pub reacting_to: Option<String>,
    /// When the line was said, not when Chat later opened. Replay stamps from this.
    pub at: SystemTime,
}

#[derive(Default)]
pub struct Log {
    turns: BTreeMap<String, Vec<Turn>>,
}

impl Log {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn remember_you(&mut self, instance: &str, text: impl Into<String>, at: SystemTime) {
        self.turns
            .entry(instance.to_string())
            .or_default()
            .push(Turn {
                you: true,
                said: Some(text.into()),
                reacting_to: None,
                at,
            });
    }

    /// Behavior-only wakes have no words; ADR-0018 holds nothing for those.
    pub fn remember_them(
        &mut self,
        instance: &str,
        said: Option<String>,
        reacting_to: Option<String>,
        at: SystemTime,
    ) {
        let Some(said) = said else {
            return;
        };
        self.turns
            .entry(instance.to_string())
            .or_default()
            .push(Turn {
                you: false,
                said: Some(said),
                reacting_to,
                at,
            });
    }

    pub fn replay(&self, instance: &str) -> Vec<Turn> {
        self.turns.get(instance).cloned().unwrap_or_default()
    }

    /// Drop what one Instance's replaced session said.
    ///
    /// One Instance, not all of them: a Character switch replaces the session
    /// behind that buddy alone, and wiping the others would empty windows whose
    /// session is still standing. #476.
    pub fn forget(&mut self, instance: &str) {
        self.turns.remove(instance);
    }

    /// Drop one Instance's turns, leaving every other buddy's alone.
    ///
    /// Saving an Instance Prompt reopens that Instance's session and no other,
    /// so the whole log going with it would take a conversation the Completer
    /// still holds (ADR-0012).
    pub fn forget(&mut self, instance: &str) {
        self.turns.remove(instance);
    }
}

fn with_log(app: &tauri::AppHandle, f: impl FnOnce(&mut Log)) {
    if let Some(held) = app.try_state::<Mutex<Log>>() {
        if let Ok(mut log) = held.lock() {
            f(&mut log);
        }
    }
}

pub fn remember_you(
    app: &tauri::AppHandle,
    instance: &str,
    text: impl Into<String>,
    at: SystemTime,
) {
    with_log(app, |log| log.remember_you(instance, text, at));
}

pub fn remember_them(
    app: &tauri::AppHandle,
    instance: &str,
    said: Option<String>,
    reacting_to: Option<String>,
    at: SystemTime,
) {
    with_log(app, |log| {
        log.remember_them(instance, said, reacting_to, at)
    });
}

pub fn replay(app: &tauri::AppHandle, instance: &str) -> Vec<Turn> {
    app.try_state::<Mutex<Log>>()
        .and_then(|held| held.lock().ok().map(|log| log.replay(instance)))
        .unwrap_or_default()
}

/// A dismissed Instance's turns go with it.
///
/// No new session and nothing to tell: the window is closed in the same breath,
/// and the id is never handed out again. Its own call because the blanket wipe
/// that used to sweep these on the next Retarget is gone — one Instance's new
/// session must not empty another's window.
pub fn forget(app: &tauri::AppHandle, instance: &str) {
    with_log(app, |log| log.forget(instance));
}

/// The Completer session behind `instance` was replaced, for the reason `why`.
///
/// Three things at once because they are one fact: the held turns go, an open
/// surface is told, and the Action Log takes the boundary so what leaves the
/// window is not lost. `chat.js` carries the argument for all three. #476.
///
/// Called beside `model::retarget_model`, which is where a session is actually
/// replaced; the two sites that call one call the other.
pub fn new_session(app: &tauri::AppHandle, instance: &str, why: &str) {
    forget(app, instance);
    crate::action_log::append(
        &ai_buddy_core::memory::data_dir(),
        "session",
        serde_json::json!({ "instance": instance, "why": why }),
    );
    let _ = app.emit_to(crate::chat_label(instance), crate::CHAT_SESSION_EVENT, why);
}

pub fn forget(app: &tauri::AppHandle, instance: &str) {
    with_log(app, |log| log.forget(instance));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    /// Production change that would fail this: dropping a spoken line because
    /// no Chat surface was listening. ADR-0018 puts every Speech in that log.
    #[test]
    fn a_spoken_line_is_still_there_when_chat_opens_later() {
        let mut log = Log::new();
        log.remember_them(
            "buddy-1",
            Some("hello from the bubble".into()),
            Some("when poked".into()),
            UNIX_EPOCH,
        );

        let turns = log.replay("buddy-1");
        assert_eq!(turns.len(), 1);
        assert!(!turns[0].you);
        assert_eq!(turns[0].said.as_deref(), Some("hello from the bubble"));
        assert_eq!(turns[0].reacting_to.as_deref(), Some("when poked"));
    }

    /// Production change that would fail this: replaying Instance A's Speech
    /// into Instance B's window.
    #[test]
    fn replay_is_the_instance_that_said_it() {
        let mut log = Log::new();
        log.remember_them(
            "a",
            Some("from A".into()),
            Some("unprompted".into()),
            UNIX_EPOCH,
        );
        log.remember_them(
            "b",
            Some("from B".into()),
            Some("when summoned".into()),
            UNIX_EPOCH,
        );

        let a = log.replay("a");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].said.as_deref(), Some("from A"));
        assert!(log.replay("missing").is_empty());
    }

    /// Production change that would fail this: holding a Behavior-only wake
    /// that has no words (ADR-0018: nothing is held for that).
    #[test]
    fn a_wake_with_no_speech_is_not_held() {
        let mut log = Log::new();
        log.remember_them("buddy-1", None, None, UNIX_EPOCH);
        assert!(log.replay("buddy-1").is_empty());
    }

    /// Production change that would fail this: Chat showing only the buddy's
    /// lines after a close and reopen, dropping the typed request.
    #[test]
    fn typed_and_spoken_stay_in_order() {
        let mut log = Log::new();
        log.remember_them(
            "buddy-1",
            Some("unprompted hi".into()),
            Some("unprompted".into()),
            UNIX_EPOCH,
        );
        log.remember_you("buddy-1", "what are you standing on?", UNIX_EPOCH);
        log.remember_them(
            "buddy-1",
            Some("the desktop floor".into()),
            None,
            UNIX_EPOCH,
        );

        let turns = log.replay("buddy-1");
        assert_eq!(turns.len(), 3);
        assert!(turns[0].reacting_to.is_some());
        assert!(turns[1].you);
        assert_eq!(turns[1].said.as_deref(), Some("what are you standing on?"));
        assert!(!turns[2].you);
        assert_eq!(turns[2].said.as_deref(), Some("the desktop floor"));
    }

    /// Production change that would fail this: forgetting every Instance's
    /// turns when one Instance's session is reopened. Saving an Instance
    /// Prompt reopens that Instance's session and no other (ADR-0012), and the
    /// buddy beside it is still mid-conversation.
    #[test]
    fn forgetting_one_instance_leaves_the_others_conversation() {
        let mut log = Log::new();
        log.remember_you("saved", "before the edit", UNIX_EPOCH);
        log.remember_you("other", "still talking", UNIX_EPOCH);

        log.forget("saved");

        assert!(log.replay("saved").is_empty());
        assert_eq!(log.replay("other").len(), 1);
    }

    /// Production change that would fail this: keeping turns after Retarget
    /// replaced the Completer session (#476: Chat is this session only).
    #[test]
    fn retarget_forgets_the_old_session() {
        let mut log = Log::new();
        log.remember_you("buddy-1", "old session", UNIX_EPOCH);
        log.forget("buddy-1");
        assert!(log.replay("buddy-1").is_empty());
    }

    /// Production change that would fail this: emptying every buddy's log on a
    /// Character switch, which replaces one Instance's session and leaves the
    /// rest answering out of the conversation their windows still show. #476.
    #[test]
    fn a_switched_buddy_does_not_forget_the_others() {
        let mut log = Log::new();
        log.remember_you("switched", "before the switch", UNIX_EPOCH);
        log.remember_you("untouched", "still this session", UNIX_EPOCH);

        log.forget("switched");

        assert!(log.replay("switched").is_empty());
        assert_eq!(log.replay("untouched").len(), 1);
    }

    /// Production change that would fail this: stamping a replayed line with
    /// Chat-open time instead of the instant it was said (ADR-0018: one conversation).
    #[test]
    fn replay_keeps_the_moment_the_line_was_said() {
        let mut log = Log::new();
        let first = UNIX_EPOCH + Duration::from_secs(1_000);
        let second = first + Duration::from_secs(20 * 60);
        log.remember_you("buddy-1", "typed twenty minutes ago", first);
        log.remember_them("buddy-1", Some("answered later".into()), None, second);

        let turns = log.replay("buddy-1");
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].at, first);
        assert_eq!(turns[1].at, second);
        assert_ne!(turns[0].at, turns[1].at);
        assert_ne!(turns[0].at, SystemTime::now());
        assert_ne!(turns[1].at, SystemTime::now());
    }
}
