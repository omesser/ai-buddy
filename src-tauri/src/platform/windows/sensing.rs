//! Windows activity sensing: frontmost application, idle time, display sleep.
//!
//! GetForegroundWindow for frontmost, named by its process (#197), and
//! GetLastInputInfo for idle. Display sleep via GetSystemPowerStatus is an
//! approximation: Windows has no direct "displays are off" query.

use std::time::Duration;

use ai_buddy_core::sensing::ActivitySource;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use super::process::window_owner;

pub struct WindowsActivitySource;

impl ActivitySource for WindowsActivitySource {
    /// The application name, as macOS's `localizedName` and X11's `WM_CLASS`
    /// give. This read the title bar text until #197, so the exclusion list
    /// `frame_loop` matches it against could never match on Windows — and this
    /// is the privacy path, where a miss puts the name in the Director's
    /// context after the user asked for it to stay out.
    fn frontmost_application(&self) -> Option<String> {
        // SAFETY: GetForegroundWindow takes nothing and returns either a valid
        // HWND or null, which is checked before it is used.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }

        window_owner(hwnd)
    }

    fn idle(&self) -> Duration {
        // SAFETY: LASTINPUTINFO is properly initialized with its size as
        // documented. GetLastInputInfo receives a mutable reference that lives
        // for the call and fills dwTime on success. GetTickCount takes nothing
        // and returns the current tick count. Both are documented as safe to
        // call from any thread.
        unsafe {
            let mut lii = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };

            if GetLastInputInfo(&mut lii) == 0 {
                return Duration::ZERO;
            }

            let now = GetTickCount();
            let idle_ms = now.saturating_sub(lii.dwTime);
            Duration::from_millis(idle_ms as u64)
        }
    }

    fn displays_asleep(&self) -> bool {
        // SAFETY: SYSTEM_POWER_STATUS is a struct of integers and bytes; zeroed
        // initializes it validly. GetSystemPowerStatus receives a mutable
        // reference that lives for the call and fills the fields on success.
        // The function is documented as safe to call.
        unsafe {
            let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
            if GetSystemPowerStatus(&mut status) == 0 {
                return false;
            }
            status.ACLineStatus == 0 && status.BatteryLifePercent < 5
        }
    }
}
