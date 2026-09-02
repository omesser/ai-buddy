//! X11 implementation for Linux functional parity.
//!
//! Fills the same seams as `platform::macos`: pointer state, overlay configuration,
//! window geometry, activity sensing. Wayland stays degraded by design; this is
//! X11 only.

#![cfg(all(unix, not(target_os = "macos")))]

mod connection;
mod overlay;
mod pointer;
mod sensing;
mod settings_window;
mod window_source;

pub use overlay::{configure_overlay, update_input_region};
pub use pointer::{primary_button_down, secondary_button_down};
pub use sensing::X11ActivitySource;
pub use settings_window::{refresh_if_showing as refresh_settings, show as show_settings};

pub(super) use window_source::X11WindowSource;
