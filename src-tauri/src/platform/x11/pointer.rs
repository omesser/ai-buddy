//! X11 pointer button state via XQueryPointer.
//!
//! The Shell polls this beside the cursor position each tick. XQueryPointer
//! reads the current button state without needing XI2 events or grabs.
//! This is the interim X11 fallback; #183 may make pointer events portable.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ButtonMask};

use crate::platform::ButtonsDown;

/// Both buttons (Button1 and Button3) from one XQueryPointer.
///
/// One reply carries the whole mask, so this is one function rather than a
/// predicate per button: two predicates each queried the server, and the frame
/// loop asks about both every tick — two blocking round trips for an answer one
/// of them already held (#268).
///
/// No connection, or a query the server refuses, reads as nothing held. The
/// overlay witness in `platform.rs` is the other half of the answer.
pub fn buttons_down() -> ButtonsDown {
    let Some(mask) = button_state_mask() else {
        return ButtonsDown::default();
    };
    ButtonsDown {
        primary: (mask & u16::from(ButtonMask::M1)) != 0,
        secondary: (mask & u16::from(ButtonMask::M3)) != 0,
    }
}

fn button_state_mask() -> Option<u16> {
    let display = super::connection::connection()?;
    let screen = &display.setup().roots[0];
    let reply = xproto::query_pointer(display, screen.root)
        .ok()?
        .reply()
        .ok()?;
    Some(reply.mask.into())
}
