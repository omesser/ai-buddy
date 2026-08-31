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

use ai_buddy_core::sensing::ActivitySource;
use ai_buddy_core::window_source::{Rect, WindowSource};

/// The displays as the frame loop needs to see them, from one read.
///
/// Everything here comes from `NSScreen`, which may only be asked on the main
/// thread, so the loop is served the last answer read there rather than asking
/// for its own. Gathered into one type because it is one main-thread hop.
#[derive(Clone, Debug)]
pub struct Displays {
    /// The whole frame of each display, in logical points.
    ///
    /// Whole rather than usable, because the overlay has to cover the Dock and
    /// the menu bar: a held sprite may be dragged over both, and a window that
    /// stopped at the usable edge would clip it there.
    pub frames: Vec<Rect>,
    /// The part of each display a sprite may occupy, in logical points.
    ///
    /// When the Dock's true bounds are known, the floor of its display drops
    /// to the display's own bottom edge (`floor_under_dock`): the strip the
    /// work area reserved is the Dock itself, which arrives as `dock`.
    pub usable_frames: Vec<Rect>,
    /// The Dock's true bounds, when Accessibility is granted; see
    /// `macos::dock_bounds`. `None` keeps the full-width strip.
    pub dock: Option<Rect>,
    /// The scale factor the windowing layer measures the global cursor
    /// against.
    ///
    /// It is the primary display's, whichever display the cursor is actually
    /// over: the layer takes the cursor in points and multiplies by that one
    /// factor, so that one factor is what undoes it.
    pub cursor_scale: f64,
}

impl Default for Displays {
    /// A desktop nothing has been read from yet. The scale is 1 rather than 0
    /// because it is a divisor.
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            usable_frames: Vec::new(),
            dock: None,
            cursor_scale: 1.0,
        }
    }
}

/// The last read of the displays, shared between the refresh and its readers.
#[derive(Clone, Default)]
pub struct DisplayCache(Arc<Mutex<Displays>>);

impl DisplayCache {
    /// What the main thread last saw. Stale by up to `USABLE_FRAME_REFRESH`,
    /// which is a desktop that was accurate a moment ago rather than a stall in
    /// the frame loop.
    pub fn read(&self) -> Displays {
        self.0.lock().map(|read| read.clone()).unwrap_or_default()
    }
}

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

/// Where the Free tier comes from: what the user is in, and how long since they
/// touched anything.
#[cfg(target_os = "macos")]
pub fn activity_source() -> impl ActivitySource {
    macos::MacosActivitySource
}

/// A platform that reports nothing is one where every Behavior with a trigger
/// simply never fires, which leaves the untriggered ones — a life, if a duller
/// one. The same supported degradation as the missing window geometry.
#[cfg(not(target_os = "macos"))]
pub fn activity_source() -> impl ActivitySource {
    ai_buddy_core::sensing::StubActivitySource
}

/// Where window geometry comes from.
///
/// The usable part of each display is read through Tauri rather than from the
/// platform binding beside it, because the reserved strips are the window
/// manager's answer and CoreGraphics cannot give it: it reports the Dock as a
/// window covering the whole display.
///
/// Only macOS reads windows. `docs/SPEC.md` puts Windows out of scope for v1,
/// so every other platform gets displays and no windows. What this buys is that
/// the taskbar needs no separate concept when Windows does land: Tauri fills the
/// same work area from `SPI_GETWORKAREA`.
///
/// Call this on the main thread. The work area comes from `NSScreen`, which may
/// only be asked there, so the answer is read here and again on a timer, and
/// the frame loop is served the last one. Asking AppKit from the frame loop
/// appears to work and is not allowed to: `WryHandle::available_monitors`
/// reaches through a field named `main_thread` to do it.
#[cfg(target_os = "macos")]
pub fn window_source(app: tauri::AppHandle) -> (impl WindowSource, DisplayCache) {
    let cache = DisplayCache(Arc::new(Mutex::new(read_displays(&app))));
    let refreshed = Arc::new(Mutex::new(Instant::now()));

    let source = macos::MacosWindowSource::new({
        let cache = cache.clone();
        move || {
            // Posted, not awaited: a poll that arrives while the main thread
            // is busy is served the previous answer.
            if due(&refreshed) {
                let app = app.clone();
                let cache = cache.clone();
                let _ = app.clone().run_on_main_thread(move || {
                    let read = read_displays(&app);
                    if let Ok(mut displays) = cache.0.lock() {
                        *displays = read;
                    }
                });
            }

            let displays = cache.read();
            (displays.usable_frames, displays.dock)
        }
    });

    (source, cache)
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
/// which `docs/SPEC.md` calls a supported mode rather than a failure. The
/// displays still come from Tauri, which reads them on every platform; only the
/// windows are missing.
///
/// ponytail: read once at startup rather than on a timer, so a display plugged
/// in later goes unnoticed until restart. Add the refresh when a Wayland or X11
/// build actually ships.
#[cfg(not(target_os = "macos"))]
pub fn window_source(app: tauri::AppHandle) -> (impl WindowSource, DisplayCache) {
    let cache = DisplayCache(Arc::new(Mutex::new(read_displays(&app))));
    (DisplayOnlySource(cache.clone()), cache)
}

/// Screen edges and nothing else.
///
/// `Capabilities::default()` declares no `window_geometry`, so `snapshot()`
/// clears the windows and the Engine is handed a world with a floor and walls
/// and no Perches — which is what the degraded mode is.
#[cfg(not(target_os = "macos"))]
struct DisplayOnlySource(DisplayCache);

#[cfg(not(target_os = "macos"))]
impl WindowSource for DisplayOnlySource {
    fn capabilities(&self) -> ai_buddy_core::window_source::Capabilities {
        ai_buddy_core::window_source::Capabilities::default()
    }

    fn read(&self) -> ai_buddy_core::window_source::WorldGeometry {
        ai_buddy_core::window_source::WorldGeometry {
            usable_frames: self.0.read().usable_frames,
            windows: Vec::new(),
            dock: None,
        }
    }
}

/// The displays as the windowing layer sees them right now.
///
/// Read on a timer rather than once on macOS, because the desktop changes while
/// the app runs: the Dock hides and returns, changes edge, and a display can be
/// attached or unplugged.
///
/// Portable Tauri, so it is not gated on macOS: the degraded mode needs the same
/// screen edges, and reading them anywhere is what keeps it a degradation rather
/// than a world with no floor in it.
///
/// Tauri reports a monitor in physical pixels and the Engine works in points,
/// so every number here goes in physical and comes out logical. Two of the four
/// bugs `docs/SPEC.md` lists were this conversion done wrong, so the scale
/// passed is always the scale of the monitor being converted, never the
/// primary's. The arithmetic is `window_source::in_points` and
/// `window_source::usable_frame`, where it is tested; this only asks the
/// windowing layer what it can see.
fn read_displays(app: &tauri::AppHandle) -> Displays {
    use ai_buddy_core::window_source::{floor_under_dock, in_points, usable_frame};

    let Ok(monitors) = app.available_monitors() else {
        return Displays::default();
    };

    let mut displays = Displays {
        cursor_scale: app
            .primary_monitor()
            .ok()
            .flatten()
            .map_or(1.0, |monitor| monitor.scale_factor()),
        ..Displays::default()
    };

    for monitor in monitors.iter() {
        let work = monitor.work_area();
        let frame = Rect {
            x: f64::from(monitor.position().x),
            y: f64::from(monitor.position().y),
            width: f64::from(monitor.size().width),
            height: f64::from(monitor.size().height),
        };
        let work = Rect {
            x: f64::from(work.position.x),
            y: f64::from(work.position.y),
            width: f64::from(work.size.width),
            height: f64::from(work.size.height),
        };

        displays
            .frames
            .push(in_points(frame, monitor.scale_factor()));
        displays
            .usable_frames
            .push(usable_frame(frame, work, monitor.scale_factor()));
    }

    // With the Dock's true bounds in hand, the strip its work area reserved
    // is the Dock itself: the floor of that display drops to the display's
    // own bottom edge, and the Dock rides along as a Perch.
    displays.dock = exact_dock();
    if let Some(dock) = &displays.dock {
        for (usable, frame) in displays.usable_frames.iter_mut().zip(&displays.frames) {
            *usable = floor_under_dock(*usable, *frame, dock);
        }
    }

    displays
}

/// The Dock's true bounds — macOS with Accessibility already granted, and
/// nothing anywhere else. Never prompts; see `macos::dock_bounds`.
#[cfg(target_os = "macos")]
fn exact_dock() -> Option<Rect> {
    macos::dock_bounds()
}

#[cfg(not(target_os = "macos"))]
fn exact_dock() -> Option<Rect> {
    None
}
