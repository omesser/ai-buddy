//! The seam between the Shell and whatever operating system it is running on.
//!
//! `ai-buddy-core` declares what the app needs — a `WindowSource`, an
//! `ActivitySource`, a window that refuses keyboard focus. This module picks who
//! answers. macOS answers with AppKit and CoreGraphics; every other platform
//! gets the degraded mode `docs/SPEC.md` describes, which is a supported state
//! rather than an error.
//!
//! The dispatch lives here rather than in `main.rs` so that adding a platform is
//! one edit in one file.

use ai_buddy_core::window_source::WindowSource;

#[cfg(target_os = "macos")]
mod macos;

/// Make the overlay a floating, non-activating panel.
#[cfg(target_os = "macos")]
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    macos::configure_overlay(window)
}

/// Windows is stubbed deliberately: `docs/SPEC.md` puts it out of scope for v1.
/// The plain Tauri window is what every other platform gets.
#[cfg(not(target_os = "macos"))]
pub fn configure_overlay(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Where window geometry comes from.
#[cfg(target_os = "macos")]
pub fn window_source() -> impl WindowSource {
    macos::MacosWindowSource::new()
}

/// Without window geometry the Spatial Layer degrades to screen-edge physics,
/// which `docs/SPEC.md` calls a supported mode rather than a failure.
#[cfg(not(target_os = "macos"))]
pub fn window_source() -> impl WindowSource {
    ai_buddy_core::window_source::StubWindowSource
}
