//! Windows window geometry via EnumWindows and GetWindowRect.
//! Visible application windows in z-order; owner is the process image name
//! so `owner` is an application name on every platform. Consent-free.

use std::sync::Mutex;

use ai_buddy_core::window_source::{Capabilities, Rect, WindowRect, WindowSource, WorldGeometry};

use crate::mcp_resources::WindowTitle;
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GWL_STYLE, WS_EX_TOOLWINDOW, WS_VISIBLE,
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

/// Owner plus title, same walk and order as `visible_windows`. Title is
/// `GetWindowText` only — never used as an owner fallback.
pub fn visible_window_titles() -> Vec<WindowTitle> {
    visible_windows()
        .into_iter()
        .map(|window| WindowTitle {
            title: window_title(window.id as HWND),
            owner: window.owner,
        })
        .collect()
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: hwnd is an EnumWindows window still valid for this read.
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

/// EnumWindows callback that collects visible application windows.
/// SAFETY: hwnd is valid for the call; lparam is the pointer
/// `visible_windows` passed in, still live and pointing at the Mutex.
unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &*(lparam as *const Mutex<Vec<WindowRect>>);

    if let Some(window_rect) = window_rect(hwnd) {
        if let Ok(mut list) = windows.lock() {
            list.push(window_rect);
        }
    }

    TRUE
}

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

    // No title fallback when the process cannot be named: the title was the
    // bug. "Unknown" matches no exclusion, so the Engine still gets a Perch
    // rectangle and leaks that word, never a title.
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
        title: None,
        layer: 0,
    })
}

/// Whether this window is one of our own overlay windows.
/// Overlays run in this process, so the process ID answers it and they
/// cannot block Perch detection.
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
