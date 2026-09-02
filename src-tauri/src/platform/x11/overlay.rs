//! X11 overlay window configuration: floating, non-activating, click-through.
//!
//! Sets EWMH window states to make the overlay float above other windows,
//! skip the taskbar and pager. The spec says click-through via XShapeCombineMask
//! will be implemented after window geometry, as it requires the sprite alpha mask
//! from the frame loop.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, Atom, AtomEnum, PropMode};
use x11rb::rust_connection::RustConnection;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Configure the Tauri window as an X11 overlay: floating, non-activating, skip taskbar.
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    let Ok(raw_window_handle) = window.window_handle() else {
        return Err("Failed to get window handle".to_string());
    };

    let x_window = match raw_window_handle.as_raw() {
        RawWindowHandle::Xlib(xlib_window) => xlib_window.window as u32,
        RawWindowHandle::Xcb(xcb_window) => xcb_window.window.get(),
        _ => {
            return Err("Not an X11 window (Wayland stays degraded by design)".to_string());
        }
    };

    let (conn, _screen) =
        RustConnection::connect(None).map_err(|e| format!("Failed to connect to X11: {e}"))?;

    set_ewmh_states(&conn, x_window)?;
    Ok(())
}

/// Set EWMH states: _NET_WM_STATE_ABOVE, _NET_WM_STATE_SKIP_TASKBAR, _NET_WM_STATE_SKIP_PAGER.
fn set_ewmh_states(conn: &RustConnection, window: u32) -> Result<(), String> {
    let net_wm_state = intern_atom(conn, "_NET_WM_STATE")?;
    let above = intern_atom(conn, "_NET_WM_STATE_ABOVE")?;
    let skip_taskbar = intern_atom(conn, "_NET_WM_STATE_SKIP_TASKBAR")?;
    let skip_pager = intern_atom(conn, "_NET_WM_STATE_SKIP_PAGER")?;

    let states = [above, skip_taskbar, skip_pager];
    xproto::change_property(
        conn,
        PropMode::REPLACE,
        window,
        net_wm_state,
        AtomEnum::ATOM,
        32,
        states.len() as u32,
        bytemuck::cast_slice(&states),
    )
    .map_err(|e| format!("Failed to set EWMH states: {e}"))?
    .check()
    .map_err(|e| format!("X11 error setting states: {e}"))?;

    conn.flush()
        .map_err(|e| format!("Failed to flush X11: {e}"))?;
    Ok(())
}

/// Intern an atom, reusing it if it already exists.
fn intern_atom(conn: &RustConnection, name: &str) -> Result<Atom, String> {
    xproto::intern_atom(conn, false, name.as_bytes())
        .map_err(|e| format!("Failed to intern atom {name}: {e}"))?
        .reply()
        .map(|reply| reply.atom)
        .map_err(|e| format!("Failed to get atom reply for {name}: {e}"))
}
