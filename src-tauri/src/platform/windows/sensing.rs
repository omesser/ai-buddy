//! Windows activity sensing: frontmost application and idle time.
//!
//! Uses GetForegroundWindow for the frontmost application and GetLastInputInfo
//! for idle time. Display sleep detection uses GetSystemPowerStatus, which is
//! an approximation — Windows has no direct API for "displays are off".

use std::time::Duration;

use ai_buddy_core::sensing::ActivitySource;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Threading::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

const MAX_TITLE_LENGTH: usize = 256;

pub struct WindowsActivitySource;

impl ActivitySource for WindowsActivitySource {
    fn frontmost_application(&self) -> Option<String> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }

            let mut title_buf = [0u16; MAX_TITLE_LENGTH];
            let len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), MAX_TITLE_LENGTH as i32);
            if len <= 0 {
                return None;
            }

            String::from_utf16(&title_buf[..len as usize]).ok()
        }
    }

    fn idle(&self) -> Duration {
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
        unsafe {
            let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
            if GetSystemPowerStatus(&mut status) == 0 {
                return false;
            }
            status.ACLineStatus == 0 && status.BatteryLifePercent < 5
        }
    }
}
