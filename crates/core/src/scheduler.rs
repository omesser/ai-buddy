//! Two-clock scheduler: 16ms timer when the sprite needs ticks, `recv` when it
//! does not. Hidden sleeps so XI2 cannot wake the loop. Visible Idle keeps
//! `recv` for hit-test and gesture. DND stays visible and quiet.

use crate::engine::{Frame, State};

/// How the frame loop should wait for the next tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleMode {
    /// Run the 16ms timer.
    Active,
    /// Block on `recv()` until an input event arrives.
    Idle,
}

/// Which clock the next tick uses.
/// A playing Behavior may animate while Grounded, so State alone cannot decide.
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
            "state-based check: grounded with no behavior → Idle.
             frame_loop adds animation + sleep-accrual checks to stay Active."
        );
    }

    #[test]
    fn perched_sprite_idle_is_idle() {
        let mut frame = grounded_frame();
        frame.state = State::Perched;
        assert_eq!(
            mode(&frame, true, false),
            ScheduleMode::Idle,
            "state-based check: perched with no behavior → Idle.
             frame_loop adds animation + sleep-accrual checks to stay Active."
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
