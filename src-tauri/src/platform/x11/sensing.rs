//! X11 activity sensing: frontmost application, idle time, displays asleep.
//!
//! Reads _NET_ACTIVE_WINDOW + WM_CLASS for frontmost, X11 Screensaver extension
//! (Xss) for idle duration, and DPMS for display sleep state. All consent-free,
//! same as macOS's NSWorkspace and CGEventSource APIs.

use std::sync::OnceLock;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::dpms::ConnectionExt as DpmsExt;
use x11rb::protocol::screensaver::ConnectionExt as SsExt;
use x11rb::protocol::xproto::{self, Atom, AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

/// X11 activity source for Linux.
pub struct X11ActivitySource;

impl ai_buddy_core::sensing::ActivitySource for X11ActivitySource {
    fn frontmost_application(&self) -> Option<String> {
        frontmost_window_class()
    }

    fn idle(&self) -> Duration {
        idle_duration().unwrap_or(Duration::ZERO)
    }

    fn displays_asleep(&self) -> bool {
        displays_sleeping().unwrap_or(false)
    }
}

/// Get cached X11 connection.
fn x11_connection() -> Option<&'static RustConnection> {
    static CONN: OnceLock<Option<RustConnection>> = OnceLock::new();
    CONN.get_or_init(|| RustConnection::connect(None).ok().map(|(conn, _)| conn))
        .as_ref()
}

/// Read _NET_ACTIVE_WINDOW to get the frontmost window, then WM_CLASS for its app name.
fn frontmost_window_class() -> Option<String> {
    let conn = x11_connection()?;
    let screen = &conn.setup().roots[0];
    let root = screen.root;

    let active_atom = intern_atom(conn, "_NET_ACTIVE_WINDOW").ok()?;
    let reply = xproto::get_property(conn, false, root, active_atom, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;

    if reply.format != 32 || reply.value.len() != 4 {
        return None;
    }

    let active_window = u32::from_ne_bytes([
        reply.value[0],
        reply.value[1],
        reply.value[2],
        reply.value[3],
    ]);

    window_class(conn, active_window)
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

/// Read idle duration from X11 Screensaver extension.
fn idle_duration() -> Option<Duration> {
    let conn = x11_connection()?;
    let screen = &conn.setup().roots[0];

    let info = x11rb::protocol::screensaver::query_info(conn, screen.root)
        .ok()?
        .reply()
        .ok()?;

    Some(Duration::from_millis(u64::from(info.ms_since_user_input)))
}

/// Check if displays are asleep via DPMS.
fn displays_sleeping() -> Option<bool> {
    let conn = x11_connection()?;

    let info = x11rb::protocol::dpms::info(conn).ok()?.reply().ok()?;

    Some(info.state && info.power_level != x11rb::protocol::dpms::DPMSMode::ON)
}

/// Intern an atom, reusing it if it already exists.
fn intern_atom(conn: &RustConnection, name: &str) -> Result<Atom, ()> {
    xproto::intern_atom(conn, false, name.as_bytes())
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.atom)
        .ok_or(())
}
