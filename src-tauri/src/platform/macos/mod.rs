//! Every AppKit and CoreGraphics implementation the Shell provides.
//!
//! macOS's answer to what `platform` asks for. Keeping every AppKit and
//! CoreGraphics call in the Shell is what lets the core crate build and be
//! tested with no platform binding at all.

#![cfg(target_os = "macos")]

mod overlay_panel;
mod pointer;
mod window_source;

// Nothing constructs the activity source yet: the Director is its first
// consumer, in #10 and #11. The single-crate layout hid this, because a `pub`
// item in a library counts as reachable; in a binary nothing is, so the split
// turned a quiet gap into a compiler error. Keeping the implementation and
// saying why beats deleting work that lands two issues from now.
#[allow(dead_code)]
mod sensing;

pub use overlay_panel::configure_overlay;
pub use pointer::primary_button_down;
pub use window_source::MacosWindowSource;

#[allow(unused_imports)]
pub use sensing::MacosActivitySource;
