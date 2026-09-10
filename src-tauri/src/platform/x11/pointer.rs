//! X11 pointer button state via XQueryPointer.
//!
//! The Shell polls this beside the cursor position each tick. XQueryPointer
//! reads the current button state without needing XI2 events or grabs.
//! This is the interim X11 fallback; #183 may make pointer events portable.

use gtk::prelude::*;
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

/// The OS double-click interval, in milliseconds.
///
/// Reads GtkSettings gtk-double-click-time. Returns None if GTK is not
/// initialized or the query fails.
pub fn double_click_interval_ms() -> Option<u32> {
    if !gtk::is_initialized() {
        return None;
    }
    gtk::Settings::default().and_then(|settings| {
        let interval = settings.gtk_double_click_time();
        if interval > 0 {
            Some(interval as u32)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Our GTK reader must return the same value as an independent
    /// gtk-double-click-time read (both None, or the same Some).
    #[test]
    fn double_click_interval_ms_matches_gtk_settings() {
        // Settings::default() is None until GTK is up; production runs after
        // Tauri has initialized GTK. Unit tests must init themselves.
        // Without a display, gtk::init fails and Settings props panic — so only
        // read gtk-double-click-time when GTK is actually initialized.
        let _ = gtk::init();
        if !gtk::is_initialized() {
            assert_eq!(
                double_click_interval_ms(),
                None,
                "without GTK init, x11::double_click_interval_ms must return None"
            );
            return;
        }
        let independent = gtk::Settings::default().and_then(|settings| {
            let interval = settings.gtk_double_click_time();
            if interval > 0 {
                Some(interval as u32)
            } else {
                None
            }
        });
        assert_eq!(
            double_click_interval_ms(),
            independent,
            "x11::double_click_interval_ms must match gtk::Settings gtk-double-click-time"
        );
    }
}
