//! Every AppKit and CoreGraphics implementation the Shell provides.
//!
//! macOS's answer to what `platform` asks for. Keeping every AppKit and
//! CoreGraphics call in the Shell is what lets the core crate build and be
//! tested with no platform binding at all.

#![cfg(target_os = "macos")]

mod overlay_panel;
mod pointer;
mod sensing;
mod window_source;

pub use overlay_panel::configure_overlay;
pub use pointer::primary_button_down;
pub use sensing::MacosActivitySource;
pub use window_source::MacosWindowSource;
