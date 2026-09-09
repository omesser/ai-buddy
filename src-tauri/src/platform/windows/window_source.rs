//! Windows window geometry via EnumWindows and GetWindowRect.
//!
//! EnumWindows returns visible windows in z-order. Filters to normal application
//! windows (WS_VISIBLE, not WS_EX_TOOLWINDOW), reads bounds with GetWindowRect,
//! and filters own overlays by process ID. Window owner from its process's
//! image name, so `owner` is an application name on every platform (#197).
//! Consent-free, like macOS CGWindowListCopyWindowInfo and X11 _NET_CLIENT_LIST.

use std::sync::Mutex;

use ai_buddy_core::window_source::{Capabilities, Rect, WindowRect, WindowSource, WorldGeometry};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    GWL_EXSTYLE, GWL_STYLE, WS_EX_TOOLWINDOW, WS_VISIBLE,
};

use super::process::window_owner;

/// The Windows window manager's view of the desktop.
pub struct WindowsWindowSource {
    /// Where the usable part of each display comes from. Taskbar/dock bounds
    /// on Windows come from the work area Tauri already reads.
    read_displays: Box<dyn Fn() -> (Vec<Rect>, Option<Rect>) + Send + Sync>,
}

impl WindowsWindowSource {
    pub fn new(
        read_displays: impl Fn() -> (Vec<Rect>, Option<Rect>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            read_displays: Box::new(read_displays),
        }
    }
}

impl WindowSource for WindowsWindowSource {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            window_geometry: true,
            absolute_positioning: true,
        }
    }

    fn read(&self) -> WorldGeometry {
        let (usable_frames, dock) = (self.read_displays)();
        let windows = visible_windows();

        if std::env::var("AI_BUDDY_TRACE_WINDOWS").is_ok() {
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("window_source: {} visible windows", windows.len());
                for (i, w) in windows.iter().take(3).enumerate() {
                    eprintln!(
                        "  [{}] owner={}, bounds=({},{})@{}×{}",
                        i, w.owner, w.bounds.x, w.bounds.y, w.bounds.width, w.bounds.height
                    );
                }
            }
        }

        WorldGeometry {
            usable_frames,
            windows,
            dock,
        }
    }
}

/// Visible application windows, frontmost first (z-order).
///
/// EnumWindows returns windows in z-order, top to bottom, which matches the
/// Engine's expectation. Filters to visible, non-tool windows with WS_VISIBLE
/// and without WS_EX_TOOLWINDOW.
fn visible_windows() -> Vec<WindowRect> {
    let windows: Mutex<Vec<WindowRect>> = Mutex::new(Vec::new());

    // SAFETY: EnumWindows takes a callback and a pointer-sized parameter. The
    // callback's signature matches the required WNDENUMPROC ABI. The windows
    // reference lives until EnumWindows returns, and the callback never escapes.
    unsafe {
        EnumWindows(Some(enum_window_callback), &windows as *const _ as LPARAM);
    }

    windows.into_inner().unwrap()
}

/// EnumWindows callback that collects visible application windows.
///
/// SAFETY: EnumWindows contract guarantees hwnd is valid for the call, and
/// lparam is the pointer visible_windows passed in — still live, correctly
/// aligned, and pointing at the Mutex that owns the Vec.
unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &*(lparam as *const Mutex<Vec<WindowRect>>);

    if let Some(window_rect) = window_rect(hwnd) {
        if let Ok(mut list) = windows.lock() {
            list.push(window_rect);
        }
    }

    TRUE
}

/// Read one window's geometry and owner, or None if it should be skipped.
fn window_rect(hwnd: HWND) -> Option<WindowRect> {
    // SAFETY: hwnd comes from EnumWindows, which guarantees it is valid for
    // the callback's execution. IsWindowVisible is a simple read.
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return None;
    }

    // SAFETY: GetWindowLongW on GWL_STYLE reads the window's style bits.
    // hwnd is still valid.
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) };
    if (style & (WS_VISIBLE as i32)) == 0 {
        return None;
    }

    // SAFETY: GetWindowLongW on GWL_EXSTYLE reads the extended style bits.
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) };
    if (ex_style & (WS_EX_TOOLWINDOW as i32)) != 0 {
        return None;
    }

    // SAFETY: zeroed RECT is valid for GetWindowRect to write into — all
    // zeroes is a valid but empty rectangle.
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    // SAFETY: GetWindowRect writes into the out-pointer rect, which lives
    // until this function returns.
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }

    if rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }

    if is_own_overlay(hwnd) {
        return None;
    }

    // No fallback to the window title when the process cannot be named: the
    // title was the bug (#197), and restoring it here would restore it
    // silently. "Unknown" is what X11 reports for an owner it cannot read —
    // it matches no exclusion anyone would write, so the window is reported
    // naming nobody rather than under a name the user did not exclude. macOS
    // drops such a window instead, and keeping it is the deliberate half of
    // the trade: the Engine still needs the rectangle to Perch on and to
    // occlude with, and what a nameless row leaks is that rectangle and the
    // word "Unknown", never a title.
    let owner = window_owner(hwnd).unwrap_or_else(|| "Unknown".to_string());

    Some(WindowRect {
        id: hwnd as u64,
        bounds: Rect {
            x: f64::from(rect.left),
            y: f64::from(rect.top),
            width: f64::from(rect.right - rect.left),
            height: f64::from(rect.bottom - rect.top),
        },
        owner,
        layer: 0,
    })
}

/// Whether this window is one of our own overlay windows.
///
/// ai-buddy's overlays run in this process, so the process ID answers it. This
/// prevents our overlays from blocking Perch detection.
fn is_own_overlay(hwnd: HWND) -> bool {
    let mut window_pid: u32 = 0;
    // SAFETY: GetWindowThreadProcessId writes the process ID into the
    // out-pointer window_pid, which lives until this function returns.
    // hwnd is still valid from EnumWindows.
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut window_pid);
    }
    let current_pid = std::process::id();
    window_pid == current_pid
}
