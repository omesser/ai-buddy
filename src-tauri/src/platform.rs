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

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ai_buddy_core::window_source::WindowSource;

/// How often the reserved strips are re-read.
///
/// They move at human speed — someone toggles Dock hiding, drags it to another
/// edge, or plugs a display in — so this is far more often than it needs to be
/// and still costs one main-thread hop every other poll.
#[cfg(target_os = "macos")]
const USABLE_FRAME_REFRESH: Duration = Duration::from_millis(500);

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

/// Whether the primary mouse button is down.
///
/// Polled beside the cursor rather than delivered as an event, so a drag that
/// outruns the sprite keeps being seen. See `macos::pointer`.
#[cfg(target_os = "macos")]
pub fn primary_button_down() -> bool {
    macos::primary_button_down()
}

/// Without a way to read the button there are no interaction verbs, and the
/// sprite is watched rather than touched. A supported degradation, like the
/// missing window geometry beside it.
#[cfg(not(target_os = "macos"))]
pub fn primary_button_down() -> bool {
    false
}

/// Where window geometry comes from.
///
/// The usable part of each display is read through Tauri rather than from the
/// platform binding beside it, because the reserved strips are the window
/// manager's answer and CoreGraphics cannot give it: it reports the Dock as a
/// window covering the whole display.
///
/// Only macOS is wired up. `docs/SPEC.md` puts Windows out of scope for v1, and
/// it still gets `StubWindowSource` and no displays at all. What this buys is
/// that the taskbar needs no separate concept when Windows does land: Tauri
/// fills the same work area from `SPI_GETWORKAREA`.
///
/// Call this on the main thread. The work area comes from `NSScreen`, which may
/// only be asked there, so the answer is read here and again on a timer, and
/// the frame loop is served the last one. Asking AppKit from the frame loop
/// appears to work and is not allowed to: `WryHandle::available_monitors`
/// reaches through a field named `main_thread` to do it.
#[cfg(target_os = "macos")]
pub fn window_source(app: tauri::AppHandle) -> impl WindowSource {
    let frames = Arc::new(Mutex::new(usable_frames(&app)));
    let refreshed = Arc::new(Mutex::new(Instant::now()));

    macos::MacosWindowSource::new(move || {
        // Posted, not awaited. A poll that arrives while the main thread is
        // busy is served the previous answer, which is a strip of screen that
        // was accurate a moment ago rather than a stall in the frame loop.
        if due(&refreshed) {
            let app = app.clone();
            let frames = Arc::clone(&frames);
            let _ = app.clone().run_on_main_thread(move || {
                let read = usable_frames(&app);
                if let Ok(mut frames) = frames.lock() {
                    *frames = read;
                }
            });
        }

        frames
            .lock()
            .map(|frames| frames.clone())
            .unwrap_or_default()
    })
}

/// Whether enough time has passed to ask the main thread again, marking it
/// asked if so.
#[cfg(target_os = "macos")]
fn due(refreshed: &Mutex<Instant>) -> bool {
    let Ok(mut refreshed) = refreshed.lock() else {
        return false;
    };
    if refreshed.elapsed() < USABLE_FRAME_REFRESH {
        return false;
    }
    *refreshed = Instant::now();
    true
}

/// Without window geometry the Spatial Layer degrades to screen-edge physics,
/// which `docs/SPEC.md` calls a supported mode rather than a failure.
#[cfg(not(target_os = "macos"))]
pub fn window_source(_app: tauri::AppHandle) -> impl WindowSource {
    ai_buddy_core::window_source::StubWindowSource
}

/// The part of each display a sprite may occupy, in logical points.
///
/// Read every poll rather than cached, because the reserved strips move while
/// the app runs: the Dock hides and returns, changes edge, and a display can be
/// attached or unplugged.
///
/// Tauri reports a monitor in physical pixels and the Engine works in points,
/// so every number here goes in physical and comes out logical. Two of the four
/// bugs `docs/SPEC.md` lists were this conversion done wrong — a union computed
/// in pixels, and one scale factor used across two displays — so the scale
/// passed is always the scale of the monitor being converted, never the
/// primary's. The arithmetic is `window_source::usable_frame`, where it is
/// tested; this only asks the windowing layer what it can see.
#[cfg(target_os = "macos")]
fn usable_frames(app: &tauri::AppHandle) -> Vec<ai_buddy_core::window_source::Rect> {
    use ai_buddy_core::window_source::{usable_frame, Rect};

    let Ok(monitors) = app.available_monitors() else {
        return Vec::new();
    };

    monitors
        .iter()
        .map(|monitor| {
            let work = monitor.work_area();
            usable_frame(
                Rect {
                    x: f64::from(monitor.position().x),
                    y: f64::from(monitor.position().y),
                    width: f64::from(monitor.size().width),
                    height: f64::from(monitor.size().height),
                },
                Rect {
                    x: f64::from(work.position.x),
                    y: f64::from(work.position.y),
                    width: f64::from(work.size.width),
                    height: f64::from(work.size.height),
                },
                monitor.scale_factor(),
            )
        })
        .collect()
}
