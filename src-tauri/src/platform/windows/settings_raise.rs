//! Raise the Settings webview into the overlay's HWND_TOPMOST band.
//!
//! Overlay is HWND_TOPMOST. BringWindowToTop on a normal window cannot beat that band (#799).

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, SetForegroundWindow, SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
};

/// Move-size freeze only. Must not include `SWP_NOZORDER` or the insert is a no-op.
pub(crate) const SETTINGS_RAISE_POS_FLAGS: u32 = SWP_NOMOVE | SWP_NOSIZE;

/// Put `window` in the topmost band and order it front inside that band.
pub fn raise_settings_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let raw_window_handle = window
        .window_handle()
        .map_err(|e| format!("settings window has no native handle: {e}"))?;

    let hwnd = match raw_window_handle.as_raw() {
        RawWindowHandle::Win32(win32_window) => win32_window.hwnd.get() as HWND,
        _ => return Err("Not a Windows window handle".to_string()),
    };

    // SAFETY: hwnd is the live Settings HWND from Tauri. HWND_TOPMOST and
    // these flags are the documented z-order insert; BringWindowToTop and
    // SetForegroundWindow order it front inside that band.
    unsafe {
        if SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SETTINGS_RAISE_POS_FLAGS) == 0 {
            return Err("Failed to insert Settings into the topmost band".to_string());
        }
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOZORDER;

    #[test]
    fn settings_raise_changes_z_order() {
        assert_eq!(
            SETTINGS_RAISE_POS_FLAGS & SWP_NOZORDER,
            0,
            "SWP_NOZORDER would leave Settings under the overlay"
        );
    }
}
