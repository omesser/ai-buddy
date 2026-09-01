//! Whether the user is pressing the mouse.
//!
//! `CGEventSourceButtonState` asks the window server what the button is doing
//! right now. It is a state query and not an event tap, so it needs no
//! Accessibility permission — the same trade `sensing` makes to read how long
//! ago the last input was without reading the input itself.
//!
//! The Shell polls this beside the cursor position it already reads each tick.
//! A drag that outruns the art still has to be seen, and the overlay is
//! click-through wherever the sprite is not drawn, so the webview alone would
//! drop the button the moment the cursor left the pixels. The overlay still
//! reports the press that landed on it: this query has been seen to stay
//! false for a click our own window swallowed.

use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGMouseButton};

/// Whether the primary mouse button is down.
///
/// `CombinedSessionState` rather than the HID state, so a trackpad, a mouse and
/// a tablet all count, and so does a button held while another application is
/// frontmost — the overlay never takes focus, so that is every drag it will
/// ever see.
pub fn primary_button_down() -> bool {
    CGEventSource::button_state(
        CGEventSourceStateID::CombinedSessionState,
        CGMouseButton::Left,
    )
}

/// Whether the secondary mouse button (right-click) is down.
pub fn secondary_button_down() -> bool {
    CGEventSource::button_state(
        CGEventSourceStateID::CombinedSessionState,
        CGMouseButton::Right,
    )
}
