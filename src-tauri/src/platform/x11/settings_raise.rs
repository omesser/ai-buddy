//! Raise the Settings webview into the overlay's EWMH ABOVE band.
//!
//! Overlay REPLACE works at map time. A mapped Settings needs a `_NET_WM_STATE` client message (#799).

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// `_NET_WM_STATE_ADD`. Not an atom. The first CARDINAL of the client message.
const NET_WM_STATE_ADD: u32 = 1;
/// Source indication 1: the request comes from this application, not a pager.
const SOURCE_APPLICATION: u32 = 1;

/// Payload for a `_NET_WM_STATE` client message that ADDs `_NET_WM_STATE_ABOVE`.
pub(crate) fn net_wm_state_add_above(above_atom: u32) -> [u32; 5] {
    [NET_WM_STATE_ADD, above_atom, 0, SOURCE_APPLICATION, 0]
}

/// Ask the window manager to put `window` in the ABOVE band.
pub fn raise_settings_ewmh_above(window: &tauri::WebviewWindow) -> Result<(), String> {
    let raw_window_handle = window
        .window_handle()
        .map_err(|e| format!("settings window has no native handle: {e}"))?;

    let x_window = match raw_window_handle.as_raw() {
        RawWindowHandle::Xlib(xlib_window) => xlib_window.window as u32,
        RawWindowHandle::Xcb(xcb_window) => xcb_window.window.get(),
        _ => {
            return Err(
                "Wayland surface, not an X11 window: EWMH ABOVE is unwired for Wayland".to_string(),
            );
        }
    };

    let Some(conn) = super::connection::connection() else {
        return Err("Failed to get X11 connection".to_string());
    };
    let atoms = super::atoms::atoms().ok_or("Failed to intern EWMH atoms")?;
    let root = conn.setup().roots[0].root;
    let data = net_wm_state_add_above(atoms.net_wm_state_above);
    let event = ClientMessageEvent::new(32, x_window, atoms.net_wm_state, data);

    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
        event,
    )
    .map_err(|e| format!("Failed to send _NET_WM_STATE ADD ABOVE: {e}"))?;
    conn.flush()
        .map_err(|e| format!("Failed to flush X11: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_above_payload_is_add_not_toggle() {
        assert_eq!(net_wm_state_add_above(42), [1, 42, 0, 1, 0]);
    }
}
