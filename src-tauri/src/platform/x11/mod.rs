//! X11 implementation for Linux functional parity.
//!
//! Fills the same seams as `platform::macos`: pointer state, overlay configuration,
//! window geometry, activity sensing. Wayland stays degraded by design; this is
//! X11 only.

#![cfg(all(unix, not(target_os = "macos")))]

mod pointer;

pub use pointer::{primary_button_down, secondary_button_down};
