//! When the Character gets out of the way.
//!
//! DESIGN.md decision 8 gives ai-buddy one window level and no restacking, so
//! everything the app does to stay out of the user's way it does by
//! disappearing. Which conditions take the sprite off screen is the whole of
//! that investment, and none of it needs a window server: the Shell reads the
//! conditions, and this decides what they mean.
//!
//! One rule is the desktop's — a fullscreen application is frontmost — and one
//! is the user's, a hotkey. They differ in more than their source. A rule
//! fades, because the desktop changed and the Character will be back; the
//! hotkey is answered at once, because somebody asked.
//!
//! Two conditions people expect to find here are deliberately not rules. Being
//! quiet is not being gone, so Do Not Disturb leaves the Character on screen
//! and stops it starting things, which is #84's and the Director's. And an
//! active screen share is not one either: macOS publishes no way for an app to
//! learn that its screen is being shared, but it does let a window say it must
//! never be captured — so the Shell tells the window server that, and the
//! people on the call never see the Character while its owner keeps it. See
//! `platform::macos::overlay_panel`.

use crate::engine::Rect as WindowRect;
use crate::window_source::Rect;

/// How long a rule takes to take the Character away, and to give it back.
///
/// Long enough to read as leaving rather than as a dropped frame, short enough
/// that a fullscreen application is not presented with a ghost in the corner.
pub const FADE_MS: u32 = 200;

/// How far a window's edge may sit from a display's and still count as
/// covering it, in points.
///
/// The window server measures windows and the window manager measures displays,
/// and under a fractional scale factor the two need not divide back onto the
/// same number. A point of slack costs nothing to tell fullscreen from zoomed,
/// which differ by the depth of a menu bar.
const EDGE_TOLERANCE: f64 = 1.0;

/// What the desktop says about whether the Character belongs on screen.
///
/// One condition, named rather than passed as a bare bool, because the caller
/// reads `Desktop { fullscreen_frontmost: true }` and not `update(true)`. A
/// platform that cannot see it reports `false`, which is the same answer as a
/// desktop where it is not happening: the Character stays, and the hotkey is
/// still there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Desktop {
    pub fullscreen_frontmost: bool,
}

/// Whether the Character is on screen, and what put it there.
///
/// The two absences are not the same absence. A rule will hand the Character
/// back on its own, so it leaves gently; the hotkey was asked for, so it is
/// obeyed at once and outlasts every rule that comes and goes meanwhile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Presence {
    #[default]
    Shown,
    Faded,
    Away,
}

impl Presence {
    fn visible(self) -> bool {
        matches!(self, Presence::Shown)
    }
}

/// One instruction for the overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub visible: bool,
    /// How long the change takes. Zero is at once.
    pub fade_ms: u32,
}

/// The hide rules, and the user's standing wish over them.
#[derive(Default)]
pub struct HideRules {
    /// The user asked for the Character to go away, by hotkey or by menu. Kept
    /// apart from what is on screen because it survives every rule: a
    /// fullscreen application that comes and goes must not hand back a
    /// Character its owner sent away.
    away: bool,
    /// What the overlay must be. Asked every tick rather than announced when
    /// it changes, so a webview still loading its art when a rule fired is
    /// told the answer on the first frame it draws.
    presence: Presence,
    /// How long the move into that presence was given, so the answer above
    /// arrives late with the fade that produced it rather than a fresh one.
    fade_ms: u32,
}

impl HideRules {
    /// Flip the user's wish. Takes effect on the next `update`, so a press and
    /// the desktop it lands on are decided together rather than racing.
    pub fn toggle(&mut self) {
        self.away = !self.away;
    }

    /// What the overlay must do now, or `None` when nothing the user could see
    /// has changed.
    ///
    /// Silence is most of the answer: this is asked every tick, and an overlay
    /// told to hide sixty times a second is the flicker DESIGN.md decision 8
    /// gave up restacking to avoid.
    pub fn update(&mut self, desktop: Desktop) -> Option<Change> {
        let was = self.presence;
        self.presence = if self.away {
            Presence::Away
        } else if desktop.fullscreen_frontmost {
            Presence::Faded
        } else {
            Presence::Shown
        };

        if was.visible() == self.presence.visible() {
            return None;
        }
        self.fade_ms = fade_ms(was, self.presence);
        Some(self.presence())
    }

    /// What the overlay must be right now, whether or not this tick changed
    /// it. The hit-test asks — a Character nobody can see must not swallow the
    /// click that lands where it would be — and so does every frame sent to
    /// the renderer, because a change announced once is announced before
    /// anybody is listening.
    pub fn presence(&self) -> Change {
        Change {
            visible: self.presence.visible(),
            fade_ms: self.fade_ms,
        }
    }
}

/// How long the move from one presence to another takes.
///
/// The hotkey at either end makes it instant. Everything else is a rule, and a
/// rule fades — including the fade back in, so a Character returning from a
/// fullscreen application arrives the way it left.
fn fade_ms(from: Presence, to: Presence) -> u32 {
    if from == Presence::Away || to == Presence::Away {
        0
    } else {
        FADE_MS
    }
}

/// Whether the frontmost application window covers a whole display.
///
/// The frontmost window is the first one: a `WorldSnapshot` carries windows in
/// descending z-order and only at the ordinary application level, so the head
/// of that list is the window the user is working in.
///
/// Covering a *whole* display is what separates fullscreen from zoomed. A
/// zoomed window stops at the menu bar and the Dock, and the Character is
/// welcome to sit on top of it; a fullscreen one has taken the screen, and the
/// Character is in the way.
///
/// The frames are whole display frames, not the usable ones physics runs in:
/// the reserved strips are precisely the difference being measured.
///
/// Any display counts, including one the Character is not on: what is being
/// asked is whether the window the user is working in has taken a whole screen,
/// and a presentation on the second monitor is exactly when a companion should
/// not be anywhere.
pub fn fullscreen_frontmost(windows: &[WindowRect], frames: &[Rect]) -> bool {
    windows
        .first()
        .is_some_and(|window| frames.iter().any(|frame| covers(window, frame)))
}

/// Whether a window reaches every edge of a display, give or take the slack a
/// fractional scale factor leaves behind.
fn covers(window: &WindowRect, frame: &Rect) -> bool {
    window.x <= frame.x + EDGE_TOLERANCE
        && window.y <= frame.y + EDGE_TOLERANCE
        && window.x + window.width >= frame.x + frame.width - EDGE_TOLERANCE
        && window.y + window.height >= frame.y + frame.height - EDGE_TOLERANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fullscreen() -> Desktop {
        Desktop {
            fullscreen_frontmost: true,
        }
    }

    fn faded_out() -> Option<Change> {
        Some(Change {
            visible: false,
            fade_ms: FADE_MS,
        })
    }

    fn faded_in() -> Option<Change> {
        Some(Change {
            visible: true,
            fade_ms: FADE_MS,
        })
    }

    /// The desktop the Character spends almost all of its life on. Nothing to
    /// say means nothing said: an overlay told what it already is sixty times a
    /// second is the flicker decision 8 exists to avoid.
    #[test]
    fn a_quiet_desktop_leaves_the_character_on_screen_and_says_nothing() {
        let mut rules = HideRules::default();

        assert_eq!(rules.update(Desktop::default()), None);
        assert_eq!(rules.update(Desktop::default()), None);
        assert!(rules.presence().visible);
    }

    #[test]
    fn a_fullscreen_application_taking_the_front_fades_the_character_out() {
        let mut rules = HideRules::default();

        assert_eq!(rules.update(fullscreen()), faded_out());
        assert!(!rules.presence().visible);
        assert_eq!(rules.update(fullscreen()), None, "said once, not held");

        assert_eq!(rules.update(Desktop::default()), faded_in());
        assert!(rules.presence().visible);
    }

    /// The hotkey is somebody asking, so it is answered on the frame it lands
    /// on rather than faded through.
    #[test]
    fn the_hotkey_hides_and_shows_the_character_at_once() {
        let mut rules = HideRules::default();

        rules.toggle();
        assert_eq!(
            rules.update(Desktop::default()),
            Some(Change {
                visible: false,
                fade_ms: 0
            })
        );

        rules.toggle();
        assert_eq!(
            rules.update(Desktop::default()),
            Some(Change {
                visible: true,
                fade_ms: 0
            })
        );
    }

    /// A Character the user sent away stays away. A fullscreen application
    /// arriving and leaving must not hand it back.
    #[test]
    fn a_character_sent_away_outlasts_every_rule_that_comes_and_goes() {
        let mut rules = HideRules::default();
        rules.toggle();
        rules.update(Desktop::default());

        assert_eq!(rules.update(fullscreen()), None, "already gone");
        assert_eq!(
            rules.update(Desktop::default()),
            None,
            "the rule lifting does not undo the hotkey"
        );
        assert!(!rules.presence().visible);

        rules.toggle();
        assert_eq!(
            rules.update(Desktop::default()),
            Some(Change {
                visible: true,
                fade_ms: 0
            })
        );
    }

    /// The other order: the hotkey asks for the Character back while a rule is
    /// still holding it. Nothing happens yet, and when the rule clears the
    /// Character fades in, because the rule is what it is returning from.
    #[test]
    fn asking_for_the_character_back_under_a_rule_waits_for_the_rule() {
        let mut rules = HideRules::default();
        rules.toggle();
        rules.update(fullscreen());

        rules.toggle();
        assert_eq!(
            rules.update(fullscreen()),
            None,
            "the rule still has it, so nothing on screen changed"
        );
        assert!(!rules.presence().visible);

        assert_eq!(rules.update(Desktop::default()), faded_in());
    }

    /// The hotkey's answer is instant whatever else changed alongside it. A
    /// fullscreen application quitting and the user asking for the Character
    /// back land in the same tick easily enough, and what the user did is press
    /// the hotkey.
    #[test]
    fn the_hotkey_answers_at_once_even_when_a_rule_lifts_with_it() {
        let mut rules = HideRules::default();
        rules.update(fullscreen());

        rules.toggle();
        assert_eq!(rules.update(fullscreen()), None, "hidden either way");

        rules.toggle();
        assert_eq!(
            rules.update(Desktop::default()),
            Some(Change {
                visible: true,
                fade_ms: 0
            })
        );
    }

    /// What the hit-test reads. A Character nobody can see must not swallow
    /// the click that lands where it would have been.
    #[test]
    fn a_hidden_character_is_not_there_to_be_clicked() {
        let mut rules = HideRules::default();
        assert!(rules.presence().visible);

        rules.update(fullscreen());
        assert!(!rules.presence().visible);

        rules.update(Desktop::default());
        assert!(rules.presence().visible);
    }

    /// What the renderer is told on every tick, rather than on the tick the
    /// answer changed. A change is announced once, and the first tick lands
    /// before the webview has finished loading its art and started listening:
    /// a Character hidden then would sit on top of the fullscreen application
    /// that hid it for the rest of the session.
    #[test]
    fn the_rules_still_say_the_character_is_gone_long_after_the_rule_fired() {
        let mut rules = HideRules::default();
        assert_eq!(
            rules.presence(),
            Change {
                visible: true,
                fade_ms: 0
            },
            "on screen at the start, and not faded there"
        );

        for _ in 0..4 {
            rules.update(fullscreen());
        }
        assert_eq!(
            rules.presence(),
            Change {
                visible: false,
                fade_ms: FADE_MS
            }
        );

        for _ in 0..4 {
            rules.update(Desktop::default());
        }
        assert_eq!(
            rules.presence(),
            Change {
                visible: true,
                fade_ms: FADE_MS
            }
        );
    }

    /// The standing answer carries the fade that produced it, so a Character
    /// put away by hotkey is still put away at once on the ticks that say
    /// nothing — the renderer must not fade in what a keypress banished.
    #[test]
    fn the_standing_answer_carries_the_fade_of_the_change_that_made_it() {
        let mut rules = HideRules::default();
        rules.toggle();
        rules.update(Desktop::default());
        rules.update(Desktop::default());

        assert_eq!(
            rules.presence(),
            Change {
                visible: false,
                fade_ms: 0
            }
        );
    }

    fn display() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        }
    }

    fn window(x: f64, y: f64, width: f64, height: f64) -> WindowRect {
        WindowRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_window_covering_its_whole_display_is_a_fullscreen_application() {
        assert!(fullscreen_frontmost(
            &[window(0.0, 0.0, 1920.0, 1080.0)],
            &[display()]
        ));
    }

    /// The case that makes this worth computing. A zoomed window stops at the
    /// menu bar and the Dock, and the Character sits on its top edge as usual.
    #[test]
    fn a_zoomed_window_stops_at_the_menu_bar_and_is_not_fullscreen() {
        assert!(!fullscreen_frontmost(
            &[window(0.0, 30.0, 1920.0, 952.0)],
            &[display()]
        ));
    }

    #[test]
    fn a_fullscreen_window_that_is_not_frontmost_does_not_hide_the_character() {
        assert!(!fullscreen_frontmost(
            &[
                window(100.0, 100.0, 800.0, 600.0),
                window(0.0, 0.0, 1920.0, 1080.0),
            ],
            &[display()]
        ));
    }

    /// A display that does not begin at the origin, which is every display but
    /// the primary one. Both of its far edges are its origin plus its size, so
    /// a window matching only the size is short by the whole origin.
    fn second_display() -> Rect {
        Rect {
            x: 1920.0,
            y: 200.0,
            width: 1728.0,
            height: 1117.0,
        }
    }

    /// A second display is another whole screen an application can take, and
    /// taking it hides the Character wherever the Character is standing.
    #[test]
    fn a_fullscreen_window_on_a_second_display_counts_too() {
        assert!(fullscreen_frontmost(
            &[window(1920.0, 200.0, 1728.0, 1117.0)],
            &[display(), second_display()]
        ));
    }

    /// All four edges are measured, and reaching three of them is a window.
    /// The two on a display of its own are the ones that cost: a window is
    /// measured against where that display starts, not merely against how big
    /// it is, and dropping the origin calls a quarter-width window fullscreen
    /// and takes the Character off both screens.
    #[test]
    fn a_window_short_of_any_one_edge_is_not_a_fullscreen_application() {
        let displays = [display(), second_display()];

        // Full height and out to the right edge, but starting a long way in.
        assert!(!fullscreen_frontmost(
            &[window(200.0, 0.0, 1720.0, 1080.0)],
            &displays
        ));
        // Down to the bottom edge, but starting below the menu bar — the
        // zoomed window above stops short of the bottom as well, so without
        // this one nothing measures the top edge at all.
        assert!(!fullscreen_frontmost(
            &[window(0.0, 30.0, 1920.0, 1050.0)],
            &displays
        ));
        // Full height, pinned to the left edge, and narrow.
        assert!(!fullscreen_frontmost(
            &[window(0.0, 0.0, 400.0, 1080.0)],
            &displays
        ));
        // Narrow on the second display: its right edge is past that display's
        // width, and nowhere near its right edge at 1920 + 1728.
        assert!(!fullscreen_frontmost(
            &[window(1920.0, 200.0, 400.0, 1117.0)],
            &displays
        ));
        // Short on the second display: past 1117 points from the top of the
        // desktop, and still short of its bottom edge at 200 + 1117.
        assert!(!fullscreen_frontmost(
            &[window(1920.0, 200.0, 1728.0, 1000.0)],
            &displays
        ));
    }

    #[test]
    fn a_desktop_with_no_windows_has_no_fullscreen_application() {
        assert!(!fullscreen_frontmost(&[], &[display()]));
        assert!(!fullscreen_frontmost(
            &[window(0.0, 0.0, 1920.0, 1080.0)],
            &[]
        ));
    }

    /// A fractional scale factor divides a window's edges and a display's into
    /// numbers that need not land on each other. Being a hair short is still
    /// fullscreen; being a menu bar short is not.
    #[test]
    fn edges_a_hair_apart_are_still_the_whole_display() {
        assert!(fullscreen_frontmost(
            &[window(0.3, 0.3, 1919.4, 1079.4)],
            &[display()]
        ));
        assert!(!fullscreen_frontmost(
            &[window(0.0, 0.0, 1920.0, 1077.0)],
            &[display()]
        ));
    }
}
