//! X11 activity sensing: frontmost application, idle time, displays asleep.
//!
//! Reads _NET_ACTIVE_WINDOW + WM_CLASS for frontmost, X11 Screensaver extension
//! (Xss) for idle duration, and DPMS for display sleep state. All consent-free,
//! same as macOS's NSWorkspace and CGEventSource APIs.

use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, AtomEnum};

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

/// `_NET_ACTIVE_WINDOW` for the frontmost window, then its `WM_CLASS`: the
/// class is the application name that triggers match against.
fn frontmost_window_class() -> Option<String> {
    let conn = super::connection::connection()?;
    let screen = &conn.setup().roots[0];
    let root = screen.root;

    let active_atom = super::atoms::atoms()?.net_active_window;
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

    super::atoms::window_class(conn, active_window)
}

/// Read idle duration from X11 Screensaver extension.
fn idle_duration() -> Option<Duration> {
    let conn = super::connection::connection()?;
    let screen = &conn.setup().roots[0];

    let info = x11rb::protocol::screensaver::query_info(conn, screen.root)
        .ok()?
        .reply()
        .ok()?;

    Some(Duration::from_millis(u64::from(info.ms_since_user_input)))
}

fn displays_sleeping() -> Option<bool> {
    let conn = super::connection::connection()?;

    let info = x11rb::protocol::dpms::info(conn).ok()?.reply().ok()?;

    Some(info.state && info.power_level != x11rb::protocol::dpms::DPMSMode::ON)
}
