//! Windows Win32 implementation for functional parity.
//!
//! Fills the same seams as `platform::macos` and `platform::x11`: pointer state,
//! overlay configuration, window geometry, activity sensing. Windows specific:
//! uses Win32 APIs instead of stubs where possible to match macOS/X11 behavior.

#![cfg(not(unix))]

mod overlay;
mod pointer;
mod sensing;
mod settings_window;
mod window_source;

pub use overlay::{configure_overlay, update_input_region};
pub use pointer::buttons_down;
pub use sensing::WindowsActivitySource;
pub use settings_window::{refresh_settings, show_settings};

pub(super) use window_source::WindowsWindowSource;
