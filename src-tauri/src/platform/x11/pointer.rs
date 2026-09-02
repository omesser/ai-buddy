//! X11 pointer button state via XQueryPointer.
//!
//! The Shell polls this beside the cursor position each tick. XQueryPointer
//! reads the current button state without needing XI2 events or grabs.
//! This is the interim X11 fallback; #183 may make pointer events portable.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ButtonMask};

/// Whether the primary mouse button (Button1) is down.
pub fn primary_button_down() -> bool {
    button_state_mask().is_some_and(|mask| (mask & u16::from(ButtonMask::M1)) != 0)
}

/// Whether the secondary mouse button (Button3, right-click) is down.
pub fn secondary_button_down() -> bool {
    button_state_mask().is_some_and(|mask| (mask & u16::from(ButtonMask::M3)) != 0)
}

/// Query the current pointer button mask as a raw u16.
fn button_state_mask() -> Option<u16> {
    let display = super::connection::connection()?;
    let screen = &display.setup().roots[0];
    let reply = xproto::query_pointer(display, screen.root)
        .ok()?
        .reply()
        .ok()?;
    Some(reply.mask.into())
}
