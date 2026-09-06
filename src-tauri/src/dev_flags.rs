//! Live values for the development switches, so a toggle lands without a
//! relaunch.
//!
//! A read site that reads its own environment variable cannot be turned on from
//! a window, which is the whole of #273: the value has to live somewhere both
//! the settings window and a frame-rate read site can reach. `Flag::env_value`
//! holds what an exported variable does to a switch.
//!
//! One static per switch rather than a map. The set is fixed at compile time,
//! and a static is what lets a read site load the value without a lock.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::model;
use crate::settings::Settings;

/// One boolean development switch and the variable that can own it.
pub struct Flag {
    var: &'static str,
    on: AtomicBool,
}

impl Flag {
    const fn new(var: &'static str) -> Self {
        Self {
            var,
            on: AtomicBool::new(false),
        }
    }

    /// The environment variable this switch answers to. The settings window
    /// names it in a frozen row's label.
    pub fn var(&self) -> &'static str {
        self.var
    }

    /// `Relaxed` is enough: a trace switch has nothing to synchronise with,
    /// and the read sites want the value, not an ordering against it.
    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// What the settings window shows, and what `seed` stores.
    pub fn in_force(&self, persisted: bool) -> bool {
        self.env_value().unwrap_or(persisted)
    }

    /// What the exported variable says, if it is exported.
    ///
    /// `model::env_switch` holds the vocabulary, so a Development switch and
    /// the Director's switch answer to the same words. `form::switch_row`
    /// reads the ownership half off the same call.
    fn env_value(&self) -> Option<bool> {
        model::env_switch(self.var)
    }

    /// Load the switch from `persisted`, with an exported variable winning.
    fn seed(&self, persisted: bool) {
        self.on.store(self.in_force(persisted), Ordering::Relaxed);
    }
}

pub static TRACE_FRAMES: Flag = Flag::new("AI_BUDDY_TRACE_FRAMES");
pub static TRACE_HITTEST: Flag = Flag::new("AI_BUDDY_TRACE_HITTEST");
pub static TRACE_DIRECTOR: Flag = Flag::new("AI_BUDDY_TRACE_DIRECTOR");
pub static TRACE_ENGINE: Flag = Flag::new("AI_BUDDY_TRACE_ENGINE");
/// The capture exclusion is an AppKit window property; no other platform has
/// one to drop.
#[cfg(target_os = "macos")]
pub static CAPTURABLE: Flag = Flag::new("AI_BUDDY_CAPTURABLE");

/// Completer timeout, reply cap, and first ambient wait, as the variable or
/// the file gives them.
///
/// Zero is unset, and covers a blank field and a non-numeric one alike: a
/// zero timeout could not complete, a zero cap leaves no room to answer in,
/// and a zero wait is no wait at all, so none is a value worth telling apart
/// from absent.
///
/// Numbers rather than `Flag`s. `model` still picks between the local and
/// hosted default when neither the variable nor the file says anything.
static TIMEOUT_SECS: AtomicU64 = AtomicU64::new(0);
static MAX_TOKENS: AtomicU32 = AtomicU32::new(0);
static WAKE_SECS: AtomicU64 = AtomicU64::new(0);

/// The Completer timeout in force, in seconds.
pub fn director_timeout_secs() -> Option<u64> {
    let secs = TIMEOUT_SECS.load(Ordering::Relaxed);
    (secs > 0).then_some(secs)
}

/// The reply cap in force, in tokens.
pub fn director_max_tokens() -> Option<u32> {
    let cap = MAX_TOKENS.load(Ordering::Relaxed);
    (cap > 0).then_some(cap)
}

/// The first ambient wait in force, in seconds.
pub fn director_wake_secs() -> Option<u64> {
    let secs = WAKE_SECS.load(Ordering::Relaxed);
    (secs > 0).then_some(secs)
}

/// One variable per switch on the Development tab.
fn flag_vars() -> Vec<&'static str> {
    vec![
        TRACE_FRAMES.var(),
        TRACE_HITTEST.var(),
        TRACE_DIRECTOR.var(),
        TRACE_ENGINE.var(),
        #[cfg(target_os = "macos")]
        CAPTURABLE.var(),
    ]
}

/// Every variable naming a switch, the Director's included, for the launch
/// check that each holds a value `model::env_switch` can read.
pub fn switch_vars() -> Vec<&'static str> {
    std::iter::once(model::ENABLED).chain(flag_vars()).collect()
}

/// Every variable a Development row answers to.
///
/// `model::tests::with_env` clears these under the test binary's env lock: a
/// shell that exported one would otherwise decide a frozen row or a seeded
/// value in a test that never mentions it. The Director's own switch is not
/// here — `with_env` already owns that one, and clearing it twice would undo
/// the value a caller asked for.
#[cfg(test)]
pub(crate) fn test_vars() -> Vec<&'static str> {
    flag_vars()
        .into_iter()
        .chain([model::TIMEOUT_SECS, model::MAX_TOKENS, model::WAKE_SECS])
        .collect()
}

/// Load every switch from `settings`, with an exported variable winning.
///
/// Called once at startup and again on each applied patch, so this has to be
/// idempotent and cheap. Re-reading the environment every time costs nothing
/// and keeps the precedence in one place.
pub fn seed(settings: &Settings) {
    TRACE_FRAMES.seed(settings.trace_frames);
    TRACE_HITTEST.seed(settings.trace_hittest);
    TRACE_DIRECTOR.seed(settings.trace_director);
    TRACE_ENGINE.seed(settings.trace_engine);
    #[cfg(target_os = "macos")]
    CAPTURABLE.seed(settings.capturable);
    TIMEOUT_SECS.store(
        model::env_or_file(model::TIMEOUT_SECS, &settings.director_timeout_secs)
            .trim()
            .parse()
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    MAX_TOKENS.store(
        model::env_or_file(model::MAX_TOKENS, &settings.director_max_tokens)
            .trim()
            .parse()
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    WAKE_SECS.store(
        model::env_or_file(model::WAKE_SECS, &settings.director_wake_secs)
            .trim()
            .parse()
            .unwrap_or(0),
        Ordering::Relaxed,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vocabulary `model::env_switch` holds, seen through a switch: the
    /// same words the Director's own variable answers to.
    #[test]
    fn an_exported_switch_reads_the_shared_vocabulary() {
        // `with_env` is the whole test binary's env lock; a second mutex would
        // not serialise against it, and concurrent setenv is undefined.
        model::tests::with_env(None, None, None, || {
            let persisted = Settings {
                trace_frames: true,
                ..Settings::default()
            };
            for (exported, on) in [
                (None, true),
                (Some("1"), true),
                (Some("on"), true),
                (Some("true"), true),
                (Some("0"), false),
                (Some("off"), false),
                (Some("no"), false),
                // An expansion that produced nothing is a mistake, not an
                // override, and a word no switch knows is not an off. Both
                // leave the file holding the switch.
                (Some(""), true),
                (Some("banana"), true),
            ] {
                match exported {
                    Some(value) => std::env::set_var(TRACE_FRAMES.var(), value),
                    None => std::env::remove_var(TRACE_FRAMES.var()),
                }
                seed(&persisted);
                assert_eq!(TRACE_FRAMES.is_on(), on, "exported {exported:?}");
            }
            std::env::remove_var(TRACE_FRAMES.var());
        });
    }

    #[test]
    fn seeding_takes_the_env_over_the_file() {
        model::tests::with_env(None, None, None, || {
            let off = Settings {
                trace_hittest: false,
                ..Settings::default()
            };
            std::env::set_var(TRACE_HITTEST.var(), "1");
            seed(&off);
            std::env::remove_var(TRACE_HITTEST.var());
            assert!(TRACE_HITTEST.is_on(), "the exported variable wins");

            seed(&off);
            assert!(!TRACE_HITTEST.is_on(), "the file wins with no variable set");
        });
    }

    #[test]
    fn a_patched_flag_moves_what_is_on_reports() {
        model::tests::with_env(None, None, None, || {
            seed(&Settings {
                trace_director: true,
                ..Settings::default()
            });
            assert!(TRACE_DIRECTOR.is_on());
            assert!(model::tracing(), "model::tracing reads the live flag");

            seed(&Settings::default());
            assert!(!TRACE_DIRECTOR.is_on());
        });
    }

    #[test]
    fn a_blank_number_is_unset() {
        model::tests::with_env(None, None, None, || {
            seed(&Settings {
                director_timeout_secs: String::new(),
                director_max_tokens: "not a number".to_string(),
                ..Settings::default()
            });
            assert_eq!(director_timeout_secs(), None);
            assert_eq!(director_max_tokens(), None);

            seed(&Settings {
                director_timeout_secs: "45".to_string(),
                director_max_tokens: "300".to_string(),
                ..Settings::default()
            });
            assert_eq!(director_timeout_secs(), Some(45));
            assert_eq!(director_max_tokens(), Some(300));
        });
    }

    /// The exported limit is read where the file's is, so both reach the read
    /// site through one decision rather than an if-cascade there.
    #[test]
    fn an_exported_limit_outranks_the_file() {
        model::tests::with_env(None, None, None, || {
            std::env::set_var(model::TIMEOUT_SECS, "7");
            std::env::set_var(model::MAX_TOKENS, "11");
            seed(&Settings {
                director_timeout_secs: "45".to_string(),
                director_max_tokens: "300".to_string(),
                ..Settings::default()
            });
            assert_eq!(director_timeout_secs(), Some(7));
            assert_eq!(director_max_tokens(), Some(11));

            std::env::remove_var(model::TIMEOUT_SECS);
            std::env::remove_var(model::MAX_TOKENS);
        });
    }

    /// The wait a user can now type is worth nothing if a stale process
    /// variable keeps deciding it, so the export has to win here too.
    #[test]
    fn an_exported_wake_interval_outranks_the_file() {
        model::tests::with_env(None, None, None, || {
            seed(&Settings::default());
            assert_eq!(director_wake_secs(), None, "blank is unset");

            seed(&Settings {
                director_wake_secs: "300".to_string(),
                ..Settings::default()
            });
            assert_eq!(director_wake_secs(), Some(300));

            std::env::set_var(model::WAKE_SECS, "30");
            seed(&Settings {
                director_wake_secs: "300".to_string(),
                ..Settings::default()
            });
            assert_eq!(director_wake_secs(), Some(30));

            std::env::remove_var(model::WAKE_SECS);
        });
    }
}
