//! X11 pointer button state via XQueryPointer.
//!
//! The Shell polls this beside the cursor position each tick. XQueryPointer
//! reads the current button state without needing XI2 events or grabs.
//! This is the interim X11 fallback; #183 may make pointer events portable.

use std::sync::OnceLock;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ButtonMask};
use x11rb::rust_connection::RustConnection;

/// Connect to the X11 display once and cache it globally.
///
/// A connection per query would race the X server startup on a cold boot.
/// OnceLock for one-time init that persists for the process lifetime.
static DISPLAY: OnceLock<Option<RustConnection>> = OnceLock::new();

fn display() -> Option<&'static RustConnection> {
    DISPLAY
        .get_or_init(|| RustConnection::connect(None).ok().map(|(conn, _)| conn))
        .as_ref()
}

/// Whether the primary mouse button (Button1) is down.
pub fn primary_button_down() -> bool {
    button_state_mask().map_or(false, |mask| (mask & u16::from(ButtonMask::M1)) != 0)
}

/// Whether the secondary mouse button (Button3, right-click) is down.
pub fn secondary_button_down() -> bool {
    button_state_mask().map_or(false, |mask| (mask & u16::from(ButtonMask::M3)) != 0)
}

/// Query the current pointer button mask as a raw u16.
fn button_state_mask() -> Option<u16> {
    let display = display()?;
    let screen = &display.setup().roots[0];
    let reply = xproto::query_pointer(display, screen.root).ok()?.reply().ok()?;
    Some(reply.mask.into())
}
