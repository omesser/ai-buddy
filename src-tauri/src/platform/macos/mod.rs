//! Every AppKit and CoreGraphics implementation the Shell provides.
//!
//! macOS's answer to what `platform` asks for. Keeping every AppKit and
//! CoreGraphics call in the Shell is what lets the core crate build and be
//! tested with no platform binding at all.

#![cfg(target_os = "macos")]

mod dock;
mod overlay_panel;
mod pointer;
mod sensing;
mod settings_window;
mod window_source;

pub use dock::dock_bounds;
pub use overlay_panel::configure_overlay;
pub use pointer::{primary_button_down, secondary_button_down};
pub use sensing::MacosActivitySource;
pub use settings_window::show as show_settings;
pub use window_source::MacosWindowSource;
