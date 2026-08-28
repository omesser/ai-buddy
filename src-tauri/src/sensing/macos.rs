//! The Free tier on macOS, without consent.
//!
//! Two APIs, chosen because neither is gated by TCC:
//!
//! - `NSWorkspace.frontmostApplication` names the application the user is in.
//!   It reports the application, never a window and never a title, so there is
//!   nothing here for Screen Recording to withhold.
//! - `CGEventSourceSecondsSinceLastEventType` reports how long ago the last
//!   input event was. It is a count of seconds, not the events themselves:
//!   reading what the user typed needs an event tap, and an event tap needs
//!   Accessibility. This module installs none.
//!
//! Anything richer — the frontmost window's title, its contents, the clipboard —
//! is the Ambient or On-Demand tier, behind consent per ADR-0005 and out of
//! scope for v1.

use std::time::Duration;

use objc2_app_kit::NSWorkspace;
use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};

use super::ActivitySource;

/// `kCGAnyInputEventType`, which the header defines as exactly this and no
/// binding exposes as a constant. Any input at all counts: a key, the mouse, the
/// trackpad, a tablet.
const ANY_INPUT_EVENT: CGEventType = CGEventType(0xFFFF_FFFF);

/// The macOS view of what the user is doing.
#[derive(Clone, Copy, Debug, Default)]
pub struct MacosActivitySource;

impl ActivitySource for MacosActivitySource {
    /// The localized application name — "Safari", not "com.apple.Safari" — since
    /// the Director's context is read by a language model and shown to the user
    /// in settings.
    fn frontmost_application(&self) -> Option<String> {
        Some(
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()?
                .localizedName()?
                .to_string(),
        )
    }

    /// Idle across the whole machine, from the HID system rather than from this
    /// session, so that input to any application counts and a locked screen
    /// still accumulates idle time.
    fn idle(&self) -> Duration {
        let seconds = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::HIDSystemState,
            ANY_INPUT_EVENT,
        );
        // Negative, infinite and NaN all mean the window server told us
        // something we cannot use. Zero is the safe reading: it says the user is
        // here, which at worst keeps the Character awake.
        Duration::try_from_secs_f64(seconds).unwrap_or(Duration::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensing::{FreeTier, SystemClock};

    /// Hand verification, deliberately not part of the suite: it needs a real
    /// window server, it reads the real clock and it sleeps, all of which
    /// `docs/SPEC.md` rules out for `cargo test`. `#[ignore]` keeps the suite
    /// pure and fast.
    ///
    /// Run it with:
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml \
    ///     sensing -- --ignored --nocapture
    /// ```
    ///
    /// It prints the Free tier once a second for ten seconds. Switch
    /// application while it runs and watch the name follow and `switched`
    /// appear on exactly that read; then take your hands off the keyboard and
    /// watch idle climb, and touch it again to watch idle drop back. What no
    /// assertion can check is the thing the issue cares about most: that no
    /// permission dialog appeared.
    #[test]
    #[ignore = "needs a real desktop; run by hand"]
    fn the_live_free_tier_follows_the_real_machine() {
        let mut tier = FreeTier::default();

        for _ in 0..10 {
            let activity = tier.read(&MacosActivitySource, &SystemClock);

            assert!(
                activity.frontmost_application.is_some(),
                "some application is always frontmost on a real desktop"
            );

            println!(
                "  idle {:>6.1}s  {}{}",
                activity.idle.as_secs_f64(),
                activity.frontmost_application.unwrap_or_default(),
                if activity.switched {
                    "  <- switched"
                } else {
                    ""
                },
            );

            std::thread::sleep(Duration::from_secs(1));
        }
    }
}
