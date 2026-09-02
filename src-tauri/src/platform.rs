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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::time::{Duration, Instant};

use ai_buddy_core::sensing::ActivitySource;
use ai_buddy_core::window_source::{Rect, WindowSource};

/// A press the overlay webview felt. `CGEventSource` is a session query and
/// has been seen to stay false for a click that landed on our own window —
/// the sprite then swallows the click and never pokes. The webview is the
/// other witness: it only hears the button while click-through is off, which
/// is exactly when the cursor is over the art.
static OVERLAY_PRIMARY: AtomicBool = AtomicBool::new(false);
static OVERLAY_SECONDARY: AtomicBool = AtomicBool::new(false);

/// Latch or release the overlay's witness of the primary button.
pub fn set_overlay_primary(down: bool) {
    OVERLAY_PRIMARY.store(down, Ordering::SeqCst);
}

/// Latch or release the overlay's witness of the secondary button.
///
/// Same reason as the primary: a right-click on our window is one
/// `CGEventSource` has been seen to miss, and without this latch the
/// webview's own menu is the only thing that hears it.
pub fn set_overlay_secondary(down: bool) {
    OVERLAY_SECONDARY.store(down, Ordering::SeqCst);
}

/// The overlay is passing clicks through, so it cannot still be holding a
/// press. A pointerup the webview never delivered would otherwise leave the
/// latch set, and `primary_button_down` would stay true after the hand had
/// gone — gluing the sprite to a button nobody is pressing.
///
/// This is the watchdog that must not look at the session poll: that poll is
/// the one that misses a press our own window swallowed, which is exactly
/// when this latch is the only witness.
pub fn overlay_passes_clicks_through() {
    OVERLAY_PRIMARY.store(false, Ordering::SeqCst);
    OVERLAY_SECONDARY.store(false, Ordering::SeqCst);
}

fn overlay_primary_down() -> bool {
    OVERLAY_PRIMARY.load(Ordering::SeqCst)
}

fn overlay_secondary_down() -> bool {
    OVERLAY_SECONDARY.load(Ordering::SeqCst)
}

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
    /// The Dock's true bounds and which source produced them; see
    /// `macos::dock_bounds` for the chain. `None` keeps the full-width strip.
    pub dock: Option<(Rect, DockSource)>,
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

/// Which rung of the Dock-geometry chain answered; see `macos::dock_bounds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // macOS-only, not used on Linux
pub enum DockSource {
    /// `CoreDockGetRect`, the private SPI: exact, no grant needed.
    CoreDock,
    /// The Accessibility API, where trust was already granted.
    Accessibility,
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
#[cfg(unix)]
const USABLE_FRAME_REFRESH: Duration = Duration::from_millis(500);

#[cfg(target_os = "macos")]
mod macos;

#[cfg(all(unix, not(target_os = "macos")))]
mod x11;

/// Make the overlay a floating, non-activating panel.
#[cfg(target_os = "macos")]
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    macos::configure_overlay(window)
}

/// X11 on Linux: EWMH states for floating, skip-taskbar, skip-pager.
/// Click-through via XShapeCombineMask will be added after window geometry lands.
/// Wayland offers no reliable compositor-independent way to configure these, so it
/// stays degraded.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    x11::configure_overlay(window)
}

/// Open the native settings window. Main thread only.
#[cfg(target_os = "macos")]
pub fn show_settings(session: crate::settings::SettingsSession) {
    macos::show_settings(session)
}

/// v1 settings is AppKit. Other platforms have no window until they have one.
#[cfg(not(target_os = "macos"))]
pub fn show_settings(_session: crate::settings::SettingsSession) {
    eprintln!("settings: the native window is macOS in v1");
}

/// Windows is stubbed deliberately: `docs/SPEC.md` puts it out of scope for v1.
/// The plain Tauri window is what every other platform gets.
#[cfg(not(unix))]
pub fn configure_overlay(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Update the input region for the overlay window based on the sprite's alpha mask.
///
/// On X11, XShapeCombineMask carves the click-through region from the sprite's
/// alpha. On macOS and other platforms, this is a no-op since Tauri's
/// `set_ignore_cursor_events` is sufficient.
///
/// Integration seam: the X11 implementation exists but is not yet wired to the
/// frame loop. See platform/x11/overlay.rs::update_input_region.
#[allow(dead_code)]
#[cfg(all(unix, not(target_os = "macos")))]
pub fn update_input_region(
    window: &tauri::WebviewWindow,
    mask_data: Option<&ai_buddy_core::overlay::AlphaMask>,
    sprite_x: i32,
    sprite_y: i32,
    scale: i32,
) -> Result<(), String> {
    x11::update_input_region(window, mask_data, sprite_x, sprite_y, scale)
}

#[allow(dead_code)]
#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn update_input_region(
    _window: &tauri::WebviewWindow,
    _mask_data: Option<&ai_buddy_core::overlay::AlphaMask>,
    _sprite_x: i32,
    _sprite_y: i32,
    _scale: i32,
) -> Result<(), String> {
    Ok(())
}

/// Whether the primary mouse button is down.
///
/// The session poll sees a drag that outruns the art. The overlay latch
/// sees a click the poll has missed on our own window. Either is a press.
#[cfg(target_os = "macos")]
pub fn primary_button_down() -> bool {
    overlay_primary_down() || macos::primary_button_down()
}

/// X11 on Linux: session poll (XQueryPointer) or overlay latch.
/// Wayland has only the overlay latch (no global pointer).
#[cfg(all(unix, not(target_os = "macos")))]
pub fn primary_button_down() -> bool {
    overlay_primary_down() || x11::primary_button_down()
}

/// Without a session poll there is only the overlay latch. A click that
/// reaches the webview still pokes; one that never does is the supported
/// degradation, like the missing window geometry beside it.
#[cfg(not(unix))]
pub fn primary_button_down() -> bool {
    overlay_primary_down()
}

/// Whether the secondary mouse button (right-click) is down.
#[cfg(target_os = "macos")]
pub fn secondary_button_down() -> bool {
    overlay_secondary_down() || macos::secondary_button_down()
}

/// X11 on Linux: XQueryPointer for Button3 (right-click).
#[cfg(all(unix, not(target_os = "macos")))]
pub fn secondary_button_down() -> bool {
    overlay_secondary_down() || x11::secondary_button_down()
}

#[cfg(not(unix))]
pub fn secondary_button_down() -> bool {
    overlay_secondary_down()
}

/// Where the Free tier comes from: what the user is in, and how long since they
/// touched anything.
#[cfg(target_os = "macos")]
pub fn activity_source() -> impl ActivitySource {
    macos::MacosActivitySource
}

/// X11 on Linux: _NET_ACTIVE_WINDOW for frontmost, Xss for idle, DPMS for sleep.
/// Wayland offers no global state, so it stays StubActivitySource.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn activity_source() -> LinuxActivitySource {
    if std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_err() {
        LinuxActivitySource::Wayland
    } else {
        LinuxActivitySource::X11(x11::X11ActivitySource)
    }
}

/// Runtime dispatch between X11 and Wayland activity sources on Linux.
#[cfg(all(unix, not(target_os = "macos")))]
pub enum LinuxActivitySource {
    X11(x11::X11ActivitySource),
    Wayland,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl ActivitySource for LinuxActivitySource {
    fn frontmost_application(&self) -> Option<String> {
        match self {
            Self::X11(source) => source.frontmost_application(),
            Self::Wayland => None,
        }
    }

    fn idle(&self) -> std::time::Duration {
        match self {
            Self::X11(source) => source.idle(),
            Self::Wayland => std::time::Duration::ZERO,
        }
    }

    fn displays_asleep(&self) -> bool {
        match self {
            Self::X11(source) => source.displays_asleep(),
            Self::Wayland => false,
        }
    }
}

/// A platform that reports nothing is one where every Behavior with a trigger
/// simply never fires, which leaves the untriggered ones — a life, if a duller
/// one. The same supported degradation as the missing window geometry.
#[cfg(not(unix))]
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
            (
                displays.usable_frames,
                displays.dock.map(|(bounds, _)| bounds),
            )
        }
    });

    (source, cache)
}

/// X11 on Linux: read windows from _NET_CLIENT_LIST, with 500ms refresh for hot-plug.
/// Wayland stays DisplayOnlySource: no global window list.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn window_source(app: tauri::AppHandle) -> (LinuxWindowSource, DisplayCache) {
    if std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_err() {
        let cache = DisplayCache(Arc::new(Mutex::new(read_displays(&app))));
        return (
            LinuxWindowSource::Wayland(DisplayOnlySource(cache.clone())),
            cache,
        );
    }

    let cache = DisplayCache(Arc::new(Mutex::new(read_displays(&app))));
    let refreshed = Arc::new(Mutex::new(Instant::now()));

    let source = x11::X11WindowSource::new({
        let cache = cache.clone();
        let app_clone = app.clone();
        move || {
            if due(&refreshed) {
                *cache.0.lock().unwrap() = read_displays(&app_clone);
            }

            let displays = cache.read();
            (
                displays.usable_frames,
                displays.dock.map(|(bounds, _)| bounds),
            )
        }
    });

    (LinuxWindowSource::X11(source), cache)
}

/// Runtime dispatch between X11 and Wayland window sources on Linux.
#[cfg(all(unix, not(target_os = "macos")))]
pub enum LinuxWindowSource {
    X11(x11::X11WindowSource),
    Wayland(DisplayOnlySource),
}

#[cfg(all(unix, not(target_os = "macos")))]
impl WindowSource for LinuxWindowSource {
    fn capabilities(&self) -> ai_buddy_core::window_source::Capabilities {
        match self {
            Self::X11(source) => source.capabilities(),
            Self::Wayland(source) => source.capabilities(),
        }
    }

    fn read(&self) -> ai_buddy_core::window_source::WorldGeometry {
        match self {
            Self::X11(source) => source.read(),
            Self::Wayland(source) => source.read(),
        }
    }
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

/// X11 display refresh check: same as macOS but without main thread dispatch.
#[cfg(all(unix, not(target_os = "macos")))]
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
/// X11 fills window_source() above with real geometry; this is the Wayland fallback.
#[cfg(all(unix, not(target_os = "macos")))]
pub struct DisplayOnlySource(DisplayCache);

/// Screen edges and nothing else, for Wayland or when DISPLAY is unset.
///
/// `Capabilities::default()` declares no `window_geometry`, so `snapshot()`
/// clears the windows and the Engine is handed a world with a floor and walls
/// and no Perches — which is what the degraded mode is.
#[cfg(all(unix, not(target_os = "macos")))]
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

/// Windows stub: read-once, displays and no windows.
#[cfg(not(unix))]
pub fn window_source(app: tauri::AppHandle) -> (impl WindowSource, DisplayCache) {
    let cache = DisplayCache(Arc::new(Mutex::new(read_displays(&app))));
    (DisplayOnlySource(cache.clone()), cache)
}

#[cfg(not(unix))]
pub struct DisplayOnlySource(DisplayCache);

#[cfg(not(unix))]
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
    use ai_buddy_core::window_source::{floor_under_dock, in_points, plausible_dock, usable_frame};

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
    // own bottom edge, and the Dock rides along as a Perch. The claim comes
    // from an unversioned source, so it is believed only when some display's
    // work area agrees it is shaped and placed like a Dock.
    displays.dock = exact_dock().filter(|(bounds, _)| {
        displays
            .frames
            .iter()
            .zip(&displays.usable_frames)
            .any(|(frame, usable)| plausible_dock(bounds, *frame, *usable))
    });
    if let Some((dock, _)) = &displays.dock {
        for (usable, frame) in displays.usable_frames.iter_mut().zip(&displays.frames) {
            *usable = floor_under_dock(*usable, *frame, dock);
        }
    }

    displays
}

/// The Dock's true bounds — macOS, over the SPI-then-Accessibility chain,
/// and nothing anywhere else. Never prompts; see `macos::dock_bounds`.
#[cfg(target_os = "macos")]
fn exact_dock() -> Option<(Rect, DockSource)> {
    macos::dock_bounds()
}

#[cfg(not(target_os = "macos"))]
fn exact_dock() -> Option<(Rect, DockSource)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A press that lands on the overlay is one `CGEventSource` has been
    /// seen to miss. The overlay's own pointer events are the other half of
    /// `primary_button_down`; without them a click on the sprite is silent.
    #[test]
    fn overlay_primary_is_enough_for_a_press() {
        set_overlay_primary(false);
        set_overlay_primary(true);
        assert!(
            primary_button_down(),
            "a click the overlay felt must count as the button down"
        );
        set_overlay_primary(false);
        // The session poll may still be true if a real button is held during
        // the test; only the overlay half is under this test's control.
    }

    /// A pointerup the webview never delivered would leave the latch set.
    /// Once the overlay is passing clicks through it cannot still be holding
    /// a press, so the latch must drop — otherwise `primary_button_down`
    /// stays true and the sprite glues to a button nobody is pressing.
    #[test]
    fn a_stale_overlay_latch_clears_when_the_overlay_passes_clicks_through() {
        set_overlay_primary(true);
        overlay_passes_clicks_through();
        assert!(
            !overlay_primary_down(),
            "click-through means the overlay is not a witness, so a lost pointerup must not keep the latch"
        );
    }

    /// A right-click on the overlay is the same miss as a left-click. Without
    /// this latch the webview's Inspect menu is the only thing that hears it.
    #[test]
    fn overlay_secondary_is_enough_for_a_press() {
        set_overlay_secondary(false);
        set_overlay_secondary(true);
        assert!(
            secondary_button_down(),
            "a right-click the overlay felt must count as the button down"
        );
        set_overlay_secondary(false);
    }

    #[test]
    fn a_stale_secondary_latch_clears_when_the_overlay_passes_clicks_through() {
        set_overlay_secondary(true);
        overlay_passes_clicks_through();
        assert!(
            !overlay_secondary_down(),
            "click-through must drop a swallowed right-click too"
        );
    }
}
