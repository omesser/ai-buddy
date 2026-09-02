//! X11 window geometry from the window manager, consent-free.
//!
//! Reads _NET_CLIENT_LIST for the window list, XGetWindowAttributes for geometry,
//! _NET_FRAME_EXTENTS for decorations, and WM_CLASS for the owner. All EWMH and ICCCM
//! properties that require no consent, exactly like macOS's CGWindowListCopyWindowInfo.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, Atom, AtomEnum, Window};
use x11rb::rust_connection::RustConnection;

use ai_buddy_core::window_source::{Capabilities, Rect, WindowRect, WindowSource, WorldGeometry};

/// The X11 window manager's view of the desktop.
pub struct X11WindowSource {
    /// Where the usable part of each display comes from, and the Dock's true
    /// bounds when a panel announces itself via _NET_WM_STRUT_PARTIAL.
    read_displays: Box<dyn Fn() -> (Vec<Rect>, Option<Rect>) + Send + Sync>,
}

impl X11WindowSource {
    pub fn new(
        read_displays: impl Fn() -> (Vec<Rect>, Option<Rect>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            read_displays: Box::new(read_displays),
        }
    }
}

impl WindowSource for X11WindowSource {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            window_geometry: true,
            absolute_positioning: true,
        }
    }

    fn read(&self) -> WorldGeometry {
        let (usable_frames, dock) = (self.read_displays)();
        WorldGeometry {
            usable_frames,
            windows: visible_windows(),
            dock: dock.or_else(strut_panel_bounds),
        }
    }
}

/// Visible windows, frontmost first.
///
/// Reads _NET_CLIENT_LIST_STACKING from the root window and reverses it:
/// X11 stacks bottom-to-top, the Engine wants frontmost first.
///
/// Filters out windows with WM_CLASS "ai-buddy"/"Ai-buddy" (our own overlays)
/// so they do not block Perch detection. The overlay covers the entire display
/// and is frontmost, so without this filter every other window's top edge would
/// be reported as hidden.
fn visible_windows() -> Vec<WindowRect> {
    let Some(conn) = super::connection::connection() else {
        return Vec::new();
    };
    let screen = &conn.setup().roots[0];
    let root = screen.root;

    let Some(windows) = window_list_stacking(conn, root) else {
        return Vec::new();
    };

    windows
        .into_iter()
        .rev()
        .filter_map(|w| window_rect(conn, w))
        .collect()
}

/// Read _NET_CLIENT_LIST_STACKING: windows in stacking order, bottom to top.
fn window_list_stacking(conn: &RustConnection, root: Window) -> Option<Vec<Window>> {
    let stacking_atom = intern_atom(conn, "_NET_CLIENT_LIST_STACKING").ok()?;
    let reply = xproto::get_property(
        conn,
        false,
        root,
        stacking_atom,
        AtomEnum::WINDOW,
        0,
        u32::MAX,
    )
    .ok()?
    .reply()
    .ok()?;

    if reply.format != 32 || reply.value.len() % 4 != 0 {
        return None;
    }

    Some(
        reply
            .value
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

/// Read one window's geometry, owner, and layer, or None if it should be skipped.
fn window_rect(conn: &RustConnection, window: Window) -> Option<WindowRect> {
    if !is_normal_window(conn, window) {
        return None;
    }

    // Skip our own overlay windows to avoid blocking Perch detection.
    // The overlay covers the entire display and is frontmost, so without this
    // filter every other window's top edge would be hidden by the overlay.
    let owner = window_class(conn, window).unwrap_or_else(|| "Unknown".to_string());
    if owner == "Ai-buddy" || owner == "ai-buddy" {
        return None;
    }

    let geom = xproto::get_geometry(conn, window).ok()?.reply().ok()?;
    let translated = xproto::translate_coordinates(conn, window, geom.root, 0, 0)
        .ok()?
        .reply()
        .ok()?;

    let (x, y, width, height) = frame_geometry(
        conn,
        window,
        translated.dst_x,
        translated.dst_y,
        geom.width,
        geom.height,
    );

    Some(WindowRect {
        id: u64::from(window),
        bounds: Rect {
            x: f64::from(x),
            y: f64::from(y),
            width: f64::from(width),
            height: f64::from(height),
        },
        owner,
        layer: 0,
    })
}

/// Check if a window is a normal application window via _NET_WM_WINDOW_TYPE.
fn is_normal_window(conn: &RustConnection, window: Window) -> bool {
    let Ok(type_atom) = intern_atom(conn, "_NET_WM_WINDOW_TYPE") else {
        return false;
    };
    let Ok(normal_atom) = intern_atom(conn, "_NET_WM_WINDOW_TYPE_NORMAL") else {
        return false;
    };

    let reply = match xproto::get_property(conn, false, window, type_atom, AtomEnum::ATOM, 0, 32)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    {
        Some(r) => r,
        None => return true,
    };

    if reply.value.is_empty() {
        return true;
    }

    if reply.format != 32 {
        return false;
    }

    reply
        .value
        .chunks_exact(4)
        .any(|chunk| u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) == normal_atom)
}

/// Read _NET_FRAME_EXTENTS and adjust geometry to include window decorations.
fn frame_geometry(
    conn: &RustConnection,
    window: Window,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
) -> (i16, i16, u16, u16) {
    let Ok(extents_atom) = intern_atom(conn, "_NET_FRAME_EXTENTS") else {
        return (x, y, width, height);
    };

    let reply =
        match xproto::get_property(conn, false, window, extents_atom, AtomEnum::CARDINAL, 0, 4)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        {
            Some(r) => r,
            None => return (x, y, width, height),
        };

    if reply.format != 32 || reply.value.len() != 16 {
        return (x, y, width, height);
    }

    let left = i32::from_ne_bytes([
        reply.value[0],
        reply.value[1],
        reply.value[2],
        reply.value[3],
    ]);
    let right = i32::from_ne_bytes([
        reply.value[4],
        reply.value[5],
        reply.value[6],
        reply.value[7],
    ]);
    let top = i32::from_ne_bytes([
        reply.value[8],
        reply.value[9],
        reply.value[10],
        reply.value[11],
    ]);
    let bottom = i32::from_ne_bytes([
        reply.value[12],
        reply.value[13],
        reply.value[14],
        reply.value[15],
    ]);

    (
        x - left as i16,
        y - top as i16,
        (i32::from(width) + left + right) as u16,
        (i32::from(height) + top + bottom) as u16,
    )
}

/// Read WM_CLASS to get the window's application name.
fn window_class(conn: &RustConnection, window: Window) -> Option<String> {
    let reply = xproto::get_property(
        conn,
        false,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        0,
        1024,
    )
    .ok()?
    .reply()
    .ok()?;

    if reply.format != 8 {
        return None;
    }

    let value = reply.value;
    String::from_utf8(value.clone())
        .ok()
        .and_then(|s| s.split('\0').nth(1).map(|c| c.to_string()))
        .or_else(|| {
            String::from_utf8_lossy(&value)
                .split('\0')
                .next()
                .map(|s| s.to_string())
        })
}

/// Intern an atom, reusing it if it already exists.
fn intern_atom(conn: &RustConnection, name: &str) -> Result<Atom, ()> {
    xproto::intern_atom(conn, false, name.as_bytes())
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.atom)
        .ok_or(())
}

/// Read _NET_WM_STRUT_PARTIAL from dock/panel windows to find the panel bounds.
///
/// EWMH _NET_WM_STRUT_PARTIAL is 12 CARDINALs: [left, right, top, bottom,
/// left_start_y, left_end_y, right_start_y, right_end_y, top_start_x, top_end_x,
/// bottom_start_x, bottom_end_x]. For a bottom panel, bottom != 0 and
/// bottom_start_x/bottom_end_x define the horizontal span.
fn strut_panel_bounds() -> Option<Rect> {
    let conn = super::connection::connection()?;
    let screen = &conn.setup().roots[0];
    let root = screen.root;

    let windows = window_list_stacking(conn, root)?;

    for window in windows {
        if !is_dock_window(conn, window) {
            continue;
        }

        let strut = read_strut_partial(conn, window)?;

        if strut[3] > 0 {
            let _screen_width = f64::from(screen.width_in_pixels);
            let screen_height = f64::from(screen.height_in_pixels);
            let bottom_height = strut[3] as f64;
            let start_x = strut[10] as f64;
            let end_x = strut[11] as f64;

            return Some(Rect {
                x: start_x,
                y: screen_height - bottom_height,
                width: end_x - start_x,
                height: bottom_height,
            });
        }
    }

    None
}

/// Check if a window is a dock/panel via _NET_WM_WINDOW_TYPE_DOCK.
fn is_dock_window(conn: &RustConnection, window: Window) -> bool {
    let Ok(type_atom) = intern_atom(conn, "_NET_WM_WINDOW_TYPE") else {
        return false;
    };
    let Ok(dock_atom) = intern_atom(conn, "_NET_WM_WINDOW_TYPE_DOCK") else {
        return false;
    };

    let reply = match xproto::get_property(conn, false, window, type_atom, AtomEnum::ATOM, 0, 32)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    {
        Some(r) => r,
        None => return false,
    };

    if reply.format != 32 {
        return false;
    }

    reply
        .value
        .chunks_exact(4)
        .any(|chunk| u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) == dock_atom)
}

/// Read _NET_WM_STRUT_PARTIAL property as 12 u32 values.
fn read_strut_partial(conn: &RustConnection, window: Window) -> Option<[u32; 12]> {
    let strut_atom = intern_atom(conn, "_NET_WM_STRUT_PARTIAL").ok()?;

    let reply = xproto::get_property(conn, false, window, strut_atom, AtomEnum::CARDINAL, 0, 12)
        .ok()?
        .reply()
        .ok()?;

    if reply.format != 32 || reply.value.len() != 48 {
        return None;
    }

    let mut result = [0u32; 12];
    for (i, chunk) in reply.value.chunks_exact(4).enumerate() {
        result[i] = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overlay windows must be filtered out of visible_windows so they do
    /// not block Perch detection. This test documents the expected behavior:
    /// windows with WM_CLASS "ai-buddy" or "Ai-buddy" should be excluded.
    ///
    /// Cannot run without a real X11 server (no mocking layer exists), so this
    /// is an ignored test. Live X11 validation confirms Perch works after the
    /// filter was added.
    #[test]
    #[ignore = "needs real X11; live desktop validates"]
    fn overlay_windows_are_filtered_out() {
        // This test exists to document the requirement and expected behavior.
        // The actual filtering logic in window_rect checks:
        //   if owner == "Ai-buddy" || owner == "ai-buddy" { return None; }
        //
        // Live X11 desktop test (manual):
        // 1. Run ai-buddy on X11
        // 2. xprop on the overlay shows WM_CLASS "ai-buddy", "Ai-buddy"
        // 3. Slide a terminal window under the sprite's feet
        // 4. Sprite should Perch on the terminal's top edge
        //
        // Before fix: overlay blocked Perch (overlay reported as frontmost window)
        // After fix: overlay excluded, terminal edge is a Perch
    }
}
