//! X11 overlay window configuration: floating, non-activating, click-through.
//!
//! GTK has no click-through finer than the whole window, so `XShapeCombineMask`
//! carves the input region from the sprite's alpha mask. EWMH window states
//! float the overlay above other windows and skip the taskbar and pager.
//! Wayland stays degraded by design.

use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, SK};
use x11rb::protocol::xproto::{self, Atom, AtomEnum, PropMode};
use x11rb::rust_connection::RustConnection;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Float above other windows, non-activating, skip the taskbar and pager.
///
/// Returns Err when the window handle is not available yet, so the caller can
/// retry on subsequent frames once the GTK widget is realized.
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    let raw_window_handle = match window.window_handle() {
        Ok(handle) => handle,
        Err(e) => {
            return Err(format!("Window handle not available yet: {}", e));
        }
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

/// `XShapeCombineMask` sets the input region: `None` makes the entire window
/// click-through, `Some` gives clicks only to the opaque pixels.
///
/// Returns Err when the window handle is not available yet, so the caller can
/// retry on subsequent frames once the GTK widget is realized.
pub fn update_input_region(
    window: &tauri::WebviewWindow,
    mask_data: Option<&ai_buddy_core::overlay::AlphaMask>,
    sprite_x: i32,
    sprite_y: i32,
    sprite_facing: i32,
    scale: i32,
) -> Result<(), String> {
    let raw_window_handle = match window.window_handle() {
        Ok(handle) => handle,
        Err(e) => {
            return Err(format!("Window handle not available yet: {}", e));
        }
    };

    let x_window = match raw_window_handle.as_raw() {
        RawWindowHandle::Xlib(xlib_window) => xlib_window.window as u32,
        RawWindowHandle::Xcb(xcb_window) => xcb_window.window.get(),
        _ => {
            // Wayland or other: degraded by design, and an Err would retry forever.
            return Ok(());
        }
    };

    let Some(conn) = super::connection::connection() else {
        return Err("Failed to get X11 connection".to_string());
    };

    if let Some(mask) = mask_data {
        apply_input_mask(
            conn,
            x_window,
            mask,
            sprite_x,
            sprite_y,
            sprite_facing,
            scale,
        )?;
    } else {
        clear_input_region(conn, x_window)?;
    }

    Ok(())
}

/// Apply the alpha mask as the input region using XShapeCombineMask.
fn apply_input_mask(
    conn: &RustConnection,
    window: u32,
    mask: &ai_buddy_core::overlay::AlphaMask,
    sprite_x: i32,
    sprite_y: i32,
    sprite_facing: i32,
    scale: i32,
) -> Result<(), String> {
    let (width, height, opaque) = mask.raw();
    let scaled_width = width * scale;
    let scaled_height = height * scale;

    let screen = &conn.setup().roots[0];
    let pixmap = conn
        .generate_id()
        .map_err(|e| format!("Failed to generate pixmap ID: {e}"))?;

    xproto::create_pixmap(
        conn,
        1, // 1-bit depth for mask
        pixmap,
        screen.root,
        scaled_width as u16,
        scaled_height as u16,
    )
    .map_err(|e| format!("Failed to create pixmap: {e}"))?;

    let gc = conn
        .generate_id()
        .map_err(|e| format!("Failed to generate GC ID: {e}"))?;
    xproto::create_gc(conn, gc, pixmap, &Default::default())
        .map_err(|e| format!("Failed to create GC: {e}"))?;

    // Clear the pixmap to 0 (transparent). CreatePixmap contents are undefined.
    xproto::poly_fill_rectangle(
        conn,
        pixmap,
        gc,
        &[xproto::Rectangle {
            x: 0,
            y: 0,
            width: scaled_width as u16,
            height: scaled_height as u16,
        }],
    )
    .map_err(|e| format!("Failed to clear pixmap: {e}"))?;

    // Set GC foreground to 1 for opaque pixels (default is 0)
    xproto::change_gc(conn, gc, &xproto::ChangeGCAux::new().foreground(1))
        .map_err(|e| format!("Failed to set GC foreground: {e}"))?;

    // Facing < 0 mirrors the mask horizontally, matching `AlphaMask::hit` and
    // the renderer.
    let mirror = sprite_facing < 0;
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if opaque[idx] {
                let draw_x = if mirror {
                    (width - 1 - x) * scale
                } else {
                    x * scale
                };
                let scaled_y = y * scale;
                xproto::poly_fill_rectangle(
                    conn,
                    pixmap,
                    gc,
                    &[xproto::Rectangle {
                        x: draw_x as i16,
                        y: scaled_y as i16,
                        width: scale as u16,
                        height: scale as u16,
                    }],
                )
                .map_err(|e| format!("Failed to draw rectangle: {e}"))?;
            }
        }
    }

    shape::mask(
        conn,
        shape::SO::SET,
        SK::INPUT,
        window,
        sprite_x as i16,
        sprite_y as i16,
        pixmap,
    )
    .map_err(|e| format!("Failed to apply input mask: {e}"))?
    .check()
    .map_err(|e| format!("X11 error applying input mask: {e}"))?;

    xproto::free_gc(conn, gc).ok();
    xproto::free_pixmap(conn, pixmap).ok();

    conn.flush()
        .map_err(|e| format!("Failed to flush X11: {e}"))?;

    Ok(())
}

/// Clear the input region, making the entire window click-through.
fn clear_input_region(conn: &RustConnection, window: u32) -> Result<(), String> {
    shape::mask(conn, shape::SO::SET, SK::INPUT, window, 0, 0, x11rb::NONE)
        .map_err(|e| format!("Failed to clear input region: {e}"))?
        .check()
        .map_err(|e| format!("X11 error clearing input region: {e}"))?;

    conn.flush()
        .map_err(|e| format!("Failed to flush X11: {e}"))?;

    Ok(())
}

/// The overlay is not an application window, so the window manager must not treat it as one.
///
/// `_NET_WM_STATE_ABOVE`, `_NET_WM_STATE_SKIP_TASKBAR`, `_NET_WM_STATE_SKIP_PAGER`.
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
