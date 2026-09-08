//! Two-clock scheduler: active timer when animating, idle recv() when still.
//!
//! The frame loop runs at 16ms when the sprite needs it — Grab, Throw, fall,
//! walk, any playing Animation — and blocks on `recv()` when it does not: still
//! cursor, still sprite, asleep, hidden. The saving is bounded to the idle case,
//! from 60 wakeups/s to zero, and the Director and activity sensing still wake.
//!
//! This is pure logic tested in core. The `ScheduleMode` tells the loop which
//! clock to use; the loop owns the channel, the timer, and the platform input
//! source.

use crate::engine::{Frame, State};

/// How the frame loop should wait for the next tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleMode {
    /// Active: run the 16ms timer. The sprite is animating, falling, being
    /// dragged, or otherwise needs frequent ticks.
    Active,
    /// Idle: block on `recv()` until an input event or other wake source
    /// arrives. The sprite is still, hidden, or asleep.
    Idle,
}

/// Whether this tick requires active timing for the next.
///
/// Accepts the current `Frame`, whether the sprite is visible, and whether any
/// `Behavior` is playing. A playing Behavior may drive an Animation (walk,
/// talk, react) even while the sprite is Grounded, which is why State alone
/// cannot decide.
pub fn mode(frame: &Frame, visible: bool, behavior_playing: bool) -> ScheduleMode {
    if !visible {
        return ScheduleMode::Idle;
    }

    match frame.state {
        State::Falling | State::Dragged | State::Climbing => ScheduleMode::Active,
        State::Grounded | State::Perched => {
            if behavior_playing {
                ScheduleMode::Active
            } else {
                ScheduleMode::Idle
            }
        }
        State::Asleep => ScheduleMode::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grounded_frame() -> Frame {
        Frame {
            position: crate::engine::Point { x: 100.0, y: 100.0 },
            velocity: crate::engine::Point { x: 0.0, y: 0.0 },
            state: State::Grounded,
            animation: "idle",
            animation_ms: 0,
            variant_draw: 0,
            dialogue: None,
            behavior: None,
            playing_behavior: None,
            playing_primitive: None,
            riding: false,
            facing: 1.0,
            addressed: false,
            cue: None,
            refused: None,
        }
    }

    #[test]
    fn falling_sprite_is_active() {
        let mut frame = grounded_frame();
        frame.state = State::Falling;
        assert_eq!(mode(&frame, true, false), ScheduleMode::Active);
    }

    #[test]
    fn dragged_sprite_is_active() {
        let mut frame = grounded_frame();
        frame.state = State::Dragged;
        assert_eq!(mode(&frame, true, false), ScheduleMode::Active);
    }

    #[test]
    fn climbing_sprite_is_active() {
        let mut frame = grounded_frame();
        frame.state = State::Climbing;
        assert_eq!(mode(&frame, true, false), ScheduleMode::Active);
    }

    #[test]
    fn grounded_sprite_playing_behavior_is_active() {
        let frame = grounded_frame();
        assert_eq!(
            mode(&frame, true, true),
            ScheduleMode::Active,
            "a behavior may walk, talk, or react while grounded"
        );
    }

    #[test]
    fn grounded_sprite_idle_is_idle() {
        let frame = grounded_frame();
        assert_eq!(
            mode(&frame, true, false),
            ScheduleMode::Idle,
            "still sprite, still cursor, no behavior — can block on recv()"
        );
    }

    #[test]
    fn perched_sprite_idle_is_idle() {
        let mut frame = grounded_frame();
        frame.state = State::Perched;
        assert_eq!(
            mode(&frame, true, false),
            ScheduleMode::Idle,
            "standing on a perch with no behavior is still idle"
        );
    }

    #[test]
    fn asleep_sprite_is_idle() {
        let mut frame = grounded_frame();
        frame.state = State::Asleep;
        assert_eq!(mode(&frame, true, false), ScheduleMode::Idle);
    }

    #[test]
    fn hidden_sprite_is_idle() {
        let frame = grounded_frame();
        assert_eq!(
            mode(&frame, false, false),
            ScheduleMode::Idle,
            "hidden sprite never needs frequent ticks"
        );
    }

    #[test]
    fn hidden_while_falling_is_idle() {
        let mut frame = grounded_frame();
        frame.state = State::Falling;
        assert_eq!(
            mode(&frame, false, false),
            ScheduleMode::Idle,
            "hidden overrides active state — nobody is watching"
        );
    }
}
