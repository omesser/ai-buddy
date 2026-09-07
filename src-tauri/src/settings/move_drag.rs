//! Whether a Settings-window press may start a move-drag.
//!
//! The three platform windows share this so they cannot disagree on the gate. #460.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Background,
    /// A control keeps the press; a move from here would steal the click.
    Control,
}

pub fn should_begin_move(modifier_held: bool, hit: Hit) -> bool {
    modifier_held && matches!(hit, Hit::Background)
}

/// The key a platform window must see held, named as that OS names it. #460.
#[allow(dead_code)] // one OS names one variant; the other two are the other targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveModifier {
    Command,
    Super,
    Alt,
}

#[allow(dead_code)]
impl MoveModifier {
    pub const MACOS: Self = Self::Command;
    pub const LINUX: Self = Self::Super;
    pub const WINDOWS: Self = Self::Alt;
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

    #[test]
    fn platform_move_modifiers_are_command_super_and_alt() {
        assert_eq!(MoveModifier::MACOS, MoveModifier::Command);
        assert_eq!(MoveModifier::LINUX, MoveModifier::Super);
        assert_eq!(MoveModifier::WINDOWS, MoveModifier::Alt);
    }
}
