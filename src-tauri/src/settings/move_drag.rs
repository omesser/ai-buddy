//! Whether a Settings-window press may start a move-drag.
//!
//! Linux and Windows share this so they cannot disagree on the gate. macOS
//! uses the same gate after an `NSControl` hit-test. #460.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Background,
    /// A control keeps the press; a move from here would steal the click.
    Control,
}

pub fn should_begin_move(modifier_held: bool, hit: Hit) -> bool {
    modifier_held && matches!(hit, Hit::Background)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_on_background_begins_a_move() {
        assert!(should_begin_move(true, Hit::Background));
    }

    #[test]
    fn background_without_modifier_does_not_begin_a_move() {
        assert!(!should_begin_move(false, Hit::Background));
    }

    #[test]
    fn modifier_on_a_control_does_not_begin_a_move() {
        assert!(!should_begin_move(true, Hit::Control));
    }

    #[test]
    fn control_without_modifier_does_not_begin_a_move() {
        assert!(!should_begin_move(false, Hit::Control));
    }
}
