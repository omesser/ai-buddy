//! Windows pointer button state via GetAsyncKeyState.
//!
//! Polled each tick beside cursor position. GetAsyncKeyState reads current button
//! state without hooks or events, same seam as macOS CGEventSourceButtonState
//! and X11 XQueryPointer.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};

use crate::platform::ButtonsDown;

/// Both buttons from GetAsyncKeyState.
///
/// GetAsyncKeyState returns a SHORT where the high bit indicates the button is
/// currently pressed. No connection or API failure reads as nothing held. The
/// overlay witness in `platform.rs` is the other half of the answer.
pub fn buttons_down() -> ButtonsDown {
    ButtonsDown {
        primary: button_down(VK_LBUTTON.into()),
        secondary: button_down(VK_RBUTTON.into()),
    }
}

fn button_down(vk_button: i32) -> bool {
    unsafe { GetAsyncKeyState(vk_button) < 0 }
}
