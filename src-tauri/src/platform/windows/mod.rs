//! Windows Win32 implementation for functional parity.
//!
//! Fills the same seams as `platform::macos` and `platform::x11`: pointer state,
//! overlay configuration, window geometry, activity sensing. Windows specific:
//! uses Win32 APIs instead of stubs where possible to match macOS/X11 behavior.

#![cfg(not(unix))]

mod overlay;
mod pointer;
mod process;
mod sensing;
mod settings_raise;
mod window_source;

pub use overlay::{configure_overlay, update_input_region};
pub use pointer::{buttons_down, double_click_interval_ms};
pub use sensing::WindowsActivitySource;
pub(super) use settings_raise::raise_settings_window;

pub(super) use window_source::{visible_window_titles, WindowsWindowSource};
