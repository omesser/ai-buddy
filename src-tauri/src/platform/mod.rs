//! Per-platform overlay window configuration.
//!
//! Tauri already gives us transparency, always-on-top, all-Spaces membership and
//! click-through toggling. What it cannot express is a window that refuses
//! keyboard focus, which is the difference between a companion and something
//! that interrupts your typing. That part is per-platform.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::configure_overlay;

/// Every other platform gets the plain Tauri window for now. Windows is stubbed
/// deliberately: `docs/SPEC.md` puts it out of scope for v1.
#[cfg(not(target_os = "macos"))]
pub fn configure_overlay(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}
