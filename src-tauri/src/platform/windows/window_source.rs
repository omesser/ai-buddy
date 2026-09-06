//! Windows window geometry via EnumWindows and GetWindowRect.
//!
//! EnumWindows returns visible windows in z-order. Filters to normal application
//! windows (WS_VISIBLE, not WS_EX_TOOLWINDOW), reads bounds with GetWindowRect,
//! and filters own overlays by process ID. Window owner from GetWindowTextW.
//! Consent-free, like macOS CGWindowListCopyWindowInfo and X11 _NET_CLIENT_LIST.

use std::sync::Mutex;

use ai_buddy_core::window_source::{Capabilities, Rect, WindowRect, WindowSource, WorldGeometry};
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GWL_STYLE, WS_EX_TOOLWINDOW, WS_VISIBLE,
};

const MAX_TITLE_LENGTH: usize = 256;

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
    unsafe {
        let windows: Mutex<Vec<WindowRect>> = Mutex::new(Vec::new());

        EnumWindows(Some(enum_window_callback), &windows as *const _ as LPARAM);

        windows.into_inner().unwrap()
    }
}

/// EnumWindows callback that collects visible application windows.
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
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return None;
        }

        let style = GetWindowLongW(hwnd, GWL_STYLE);
        if (style & (WS_VISIBLE as i32)) == 0 {
            return None;
        }

        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if (ex_style & (WS_EX_TOOLWINDOW as i32)) != 0 {
            return None;
        }

        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }

        if rect.right <= rect.left || rect.bottom <= rect.top {
            return None;
        }

        let owner = window_title(hwnd).unwrap_or_else(|| "Unknown".to_string());

        if is_own_overlay(&owner, hwnd) {
            return None;
        }

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
}

/// Read window title as the owner identifier.
fn window_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut title_buf = [0u16; MAX_TITLE_LENGTH];
        let len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), MAX_TITLE_LENGTH as i32);
        if len <= 0 {
            return None;
        }

        String::from_utf16(&title_buf[..len as usize]).ok()
    }
}

/// Whether this window is one of our own overlay windows.
///
/// Filters by checking if the window belongs to the current process, as
/// ai-buddy's overlays run in the same process. This prevents our overlays from
/// blocking Perch detection.
fn is_own_overlay(title: &str, hwnd: HWND) -> bool {
    if title.is_empty() || title == "ai-buddy" {
        return true;
    }

    unsafe {
        let mut window_pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut window_pid);
        let current_pid = std::process::id();
        window_pid == current_pid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Our overlay windows must be filtered out so they do not block Perch
    /// detection. Empty titles or "ai-buddy" are ours, plus same-process check.
    #[test]
    fn own_overlay_is_recognized() {
        assert!(is_own_overlay("ai-buddy", std::ptr::null_mut()));
        assert!(is_own_overlay("", std::ptr::null_mut()));
    }
}
