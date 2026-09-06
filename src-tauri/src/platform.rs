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

use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ai_buddy_core::sensing::ActivitySource;
use ai_buddy_core::window_source::{Rect, WindowSource};

/// One button as the overlay webview witnesses it.
///
/// `CGEventSource` is a session query and has been seen to stay false for a
/// click that landed on our own window — the sprite then swallows the click
/// and never pokes. The webview is the other witness: it only hears the button
/// while click-through is off, which is exactly when the cursor is over the
/// art.
///
/// Two bits rather than one, because the frame loop polls and a click can
/// begin and end between two polls. The level alone reads false at both, and
/// no Poke is ever minted (#182). The edge keeps the down until a read has
/// consumed it, so a press that came and went is seen exactly once — and no
/// more than once, which is what stops a real hold from turning into a hold
/// and then a phantom Poke.
struct Witness {
    /// What the webview last reported: true from pointerdown to pointerup.
    down: AtomicBool,
    /// Set on every pointerdown, cleared by the read that consumes it.
    pressed: AtomicBool,
}

impl Witness {
    const fn new() -> Self {
        Self {
            down: AtomicBool::new(false),
            pressed: AtomicBool::new(false),
        }
    }

    /// The webview heard the button go down or up.
    fn report(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
        if down {
            self.pressed.store(true, Ordering::SeqCst);
        }
    }

    /// Whether the button is down now, or was pressed since the last call.
    ///
    /// Consuming. The edge is cleared whether or not the level is true — a
    /// bitwise `|` rather than `||`, so the `swap` runs on every read.
    fn take(&self) -> bool {
        self.down.load(Ordering::SeqCst) | self.pressed.swap(false, Ordering::SeqCst)
    }

    /// The overlay is no longer a witness, so nothing it holds can be trusted.
    fn forget(&self) {
        self.down.store(false, Ordering::SeqCst);
        self.pressed.store(false, Ordering::SeqCst);
    }
}

static OVERLAY_PRIMARY: Witness = Witness::new();
static OVERLAY_SECONDARY: Witness = Witness::new();

/// Which mouse buttons one tick found down.
///
/// Both answers in one type rather than a predicate each, because on X11 they
/// come out of a single XQueryPointer reply and asking per button was two
/// blocking round trips a tick (#268). It also puts the two consuming witness
/// reads in one place, which is where the "once per tick" contract belongs.
#[derive(Clone, Copy, Debug, Default)]
pub struct ButtonsDown {
    pub primary: bool,
    pub secondary: bool,
}

/// The overlay heard the primary button go down or up.
pub fn set_overlay_primary(down: bool) {
    OVERLAY_PRIMARY.report(down);
}

/// The overlay heard the secondary button go down or up.
///
/// Same reason as the primary: a right-click on our window is one
/// `CGEventSource` has been seen to miss, and without this witness the
/// webview's own menu is the only thing that hears it.
pub fn set_overlay_secondary(down: bool) {
    OVERLAY_SECONDARY.report(down);
}

/// The overlay is passing clicks through, so it cannot still be holding a
/// press. A pointerup the webview never delivered would otherwise leave the
/// level set, and `buttons_down` would stay true after the hand had gone —
/// gluing the sprite to a button nobody is pressing.
///
/// This is the watchdog that must not look at the session poll: that poll is
/// the one that misses a press our own window swallowed, which is exactly
/// when this witness is the only one.
pub fn overlay_passes_clicks_through() {
    OVERLAY_PRIMARY.forget();
    OVERLAY_SECONDARY.forget();
}

fn overlay_primary_down() -> bool {
    OVERLAY_PRIMARY.take()
}

fn overlay_secondary_down() -> bool {
    OVERLAY_SECONDARY.take()
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
/// and still costs at most one read every other poll.
#[cfg(unix)]
const USABLE_FRAME_REFRESH: Duration = Duration::from_millis(500);

#[cfg(target_os = "macos")]
mod macos;

#[cfg(all(unix, not(target_os = "macos")))]
mod x11;

#[cfg(not(unix))]
mod windows;

/// Whether an X server answers this process — a real X11 session, or XWayland
/// proxying for a Wayland one.
///
/// The question both Linux lane gates were reaching for. They used to read
/// `WAYLAND_DISPLAY`, which every Wayland session sets even for its XWayland
/// clients, so GNOME and KDE took the degraded lane without ever asking whether
/// the X11 path would have worked — under XWayland it does, because Mutter and
/// KWin proxy the EWMH states, the XShape input region and `query_pointer` this
/// app asks for. #266.
///
/// `connection()` caches in a `OnceLock`, so the answer costs one round trip per
/// process however many times it is asked.
#[cfg(all(unix, not(target_os = "macos")))]
fn x11_answers() -> bool {
    x11::connection().is_some()
}

/// Point GTK at its X11 backend when an X server answers.
///
/// GDK reads `GDK_BACKEND` once, when it opens the display, so this has to run
/// before GTK initializes — for a Tauri app, before the builder runs. Without
/// it a Wayland session hands `x11/overlay.rs` a Wayland `RawWindowHandle` and
/// every X11 call downstream is unreachable, whatever the lane gate decided.
///
/// Conditional rather than unconditional, and that is the load-bearing part:
/// GTK aborts when it cannot open the backend it was told to use, so forcing
/// `x11` on a Wayland session with no XWayland would trade a degraded buddy for
/// one that does not start.
///
/// A backend the user already named wins. `GDK_BACKEND=wayland` is how someone
/// asks for the degraded lane on purpose — to test it, or because XWayland
/// misbehaves on their desktop — and a preference is not ours to overwrite.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn prefer_x11_backend() {
    if std::env::var_os("GDK_BACKEND").is_none() && x11_answers() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

/// Nothing to choose: macOS and Windows do not run GTK.
#[cfg(any(target_os = "macos", not(unix)))]
pub fn prefer_x11_backend() {}

/// Make the overlay a floating, non-activating panel.
#[cfg(target_os = "macos")]
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    macos::configure_overlay(window)
}

/// X11 on Linux: EWMH states for floating, skip-taskbar, skip-pager, plus
/// per-pixel click-through via XShapeCombineMask from the sprite alpha.
///
/// On GDK's Wayland backend tao's handle is a `wl_surface`, which the X11 arm
/// does not match, so this returns Err. The input region is core Wayland and
/// unwired here. DESIGN.md decision 3.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    x11::configure_overlay(window)
}

/// Open the native settings window. Main thread only.
#[cfg(target_os = "macos")]
pub fn show_settings(session: crate::settings::SettingsSession) {
    macos::show_settings(session)
}

/// Redraw the settings window from the live roster. Main thread only.
#[cfg(target_os = "macos")]
pub fn refresh_settings() {
    macos::refresh_settings()
}

/// Nudge the menu bar icon toward the clock on first launch. Main thread only.
#[cfg(target_os = "macos")]
pub fn seed_tray_position() {
    macos::seed_status_item_position();
}

/// Resize the tray icon to standard menu bar height. Main thread only.
#[cfg(target_os = "macos")]
pub fn tune_tray_icon(tray: &tauri::tray::TrayIcon) -> Result<(), tauri::Error> {
    macos::tune_tray_icon(tray)
}

/// Open the native GTK settings window on Linux. Main thread only.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn show_settings(session: crate::settings::SettingsSession) {
    x11::show_settings(session)
}

/// Redraw the GTK settings window from the live roster. Main thread only.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn refresh_settings() {
    x11::refresh_settings()
}

/// Open the native settings window on Windows. Main thread only.
#[cfg(not(unix))]
pub fn show_settings(session: crate::settings::SettingsSession) {
    windows::show_settings(session)
}

/// Redraw the settings window from the live roster. Main thread only.
#[cfg(not(unix))]
pub fn refresh_settings() {
    windows::refresh_settings()
}

/// Hand a file the user owns to whatever the desktop opens it with.
///
/// The file is created empty first because Memory has no file until the
/// Director has something to remember, and an opener given a path that is not
/// there reports it missing instead of giving the user something to write in.
pub fn open_path(path: &Path) -> Result<(), String> {
    ensure_file(path)?;
    #[cfg(unix)]
    {
        opener(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(unix))]
    {
        opener(path)
    }
}

/// Not platform-specific, so it is written once rather than in each arm below.
fn ensure_file(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if !path.exists() {
        fs::write(path, "").map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn opener(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg(path);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn opener(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

/// Open with the default application via ShellExecuteW.
///
/// `cmd /C start` was the previous shape (#195). It works until the path holds
/// `&` or `%`: Rust's `Command` quoting is not `cmd`'s, and `%VAR%` expands
/// inside quotes. ShellExecuteW takes the path as a wide-string parameter, so
/// neither metacharacter is syntax (#255).
#[cfg(not(unix))]
fn opener(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let file = shell_execute_file_wide(path);
    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();

    // Per MSDN, a return value greater than 32 means the call succeeded.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "ShellExecuteW failed opening {} (code {})",
            path.display(),
            result as isize
        ))
    }
}

/// The NUL-terminated wide path ShellExecuteW receives. Kept as its own
/// function so tests can assert `&`, `%`, and spaces reach the API intact
/// without spawning a viewer.
#[cfg(not(unix))]
fn shell_execute_file_wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// Windows: extended window styles for floating, non-activating overlay.
#[cfg(not(unix))]
pub fn configure_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    windows::configure_overlay(window)
}

/// Update the input region for the overlay window based on the sprite's alpha mask.
///
/// On X11, XShapeCombineMask carves the click-through region from the sprite's
/// alpha. On macOS and other platforms, this is a no-op since Tauri's
/// `set_ignore_cursor_events` is sufficient.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn update_input_region(
    window: &tauri::WebviewWindow,
    mask_data: Option<&ai_buddy_core::overlay::AlphaMask>,
    sprite_x: i32,
    sprite_y: i32,
    sprite_facing: i32,
    scale: i32,
) -> Result<(), String> {
    x11::update_input_region(window, mask_data, sprite_x, sprite_y, sprite_facing, scale)
}

/// Windows: SetWindowRgn from the sprite's alpha mask for click-through.
#[cfg(not(unix))]
pub fn update_input_region(
    window: &tauri::WebviewWindow,
    mask_data: Option<&ai_buddy_core::overlay::AlphaMask>,
    sprite_x: i32,
    sprite_y: i32,
    sprite_facing: i32,
    scale: i32,
) -> Result<(), String> {
    windows::update_input_region(window, mask_data, sprite_x, sprite_y, sprite_facing, scale)
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub fn update_input_region(
    _window: &tauri::WebviewWindow,
    _mask_data: Option<&ai_buddy_core::overlay::AlphaMask>,
    _sprite_x: i32,
    _sprite_y: i32,
    _sprite_facing: i32,
    _scale: i32,
) -> Result<(), String> {
    Ok(())
}

/// Which mouse buttons are down, or were pressed since the last call.
///
/// The session poll sees a drag that outruns the art. The overlay witness
/// sees a click the poll has missed on our own window, including one that
/// began and ended between two polls. Either is a press.
///
/// A consuming read: the overlay's edges are cleared by it. The frame loop asks
/// once per tick, which is what makes "since the last call" mean "since the
/// last tick".
#[cfg(target_os = "macos")]
pub fn buttons_down() -> ButtonsDown {
    ButtonsDown {
        primary: overlay_primary_down() || macos::primary_button_down(),
        secondary: overlay_secondary_down() || macos::secondary_button_down(),
    }
}

/// X11 on Linux: one XQueryPointer for both buttons, or the overlay latch.
/// Wayland has only the overlay latch (no global pointer).
///
/// The poll runs before the latches rather than between them, so the two
/// consuming reads still happen exactly once each. It is asked even when a
/// latch would have answered, where the `||` used to skip it — one round trip
/// where the tick was paying two.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn buttons_down() -> ButtonsDown {
    let session = x11::buttons_down();
    ButtonsDown {
        primary: overlay_primary_down() || session.primary,
        secondary: overlay_secondary_down() || session.secondary,
    }
}

/// Windows: GetAsyncKeyState for both buttons, or the overlay latch.
#[cfg(not(unix))]
pub fn buttons_down() -> ButtonsDown {
    let session = windows::buttons_down();
    ButtonsDown {
        primary: overlay_primary_down() || session.primary,
        secondary: overlay_secondary_down() || session.secondary,
    }
}

/// Where the Free tier comes from: what the user is in, and how long since they
/// touched anything.
#[cfg(target_os = "macos")]
pub fn activity_source() -> impl ActivitySource {
    macos::MacosActivitySource
}

/// X11 on Linux: _NET_ACTIVE_WINDOW for frontmost, Xss for idle, DPMS for sleep.
///
/// With no X server the arm stubs. Idle has two paths, `ext-idle-notify-v1` over
/// Wayland on KWin, wlroots and COSMIC and `org.gnome.Mutter.IdleMonitor` over
/// D-Bus on GNOME, with no portable one. Frontmost and display sleep have none.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn activity_source() -> LinuxActivitySource {
    if x11_answers() {
        LinuxActivitySource::X11(x11::X11ActivitySource)
    } else {
        LinuxActivitySource::Wayland
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

/// Windows: GetForegroundWindow for frontmost, GetLastInputInfo for idle.
#[cfg(not(unix))]
pub fn activity_source() -> impl ActivitySource {
    windows::WindowsActivitySource
}

/// Where window geometry comes from.
///
/// The usable part of each display is read through Tauri rather than from the
/// platform binding beside it, because the reserved strips are the window
/// manager's answer and CoreGraphics cannot give it: it reports the Dock as a
/// window covering the whole display.
///
/// macOS, X11, and Windows all read windows. Tauri fills the work area from
/// platform APIs: NSScreen on macOS, Xinerama on X11, and SPI_GETWORKAREA on
/// Windows. The taskbar is reported as part of the work area, not as a separate
/// dock bounds.
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
/// A Wayland session with no XWayland stays DisplayOnlySource: no global window
/// list.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn window_source(app: tauri::AppHandle) -> (LinuxWindowSource, DisplayCache) {
    if !x11_answers() {
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

/// Whether enough time has passed to re-read the displays, marking them read
/// if so.
///
/// Both unix lanes throttle on the same clock and differ only in what they do
/// once it says yes: macOS posts the read to the main thread, X11 does it
/// inline. That belongs to the call sites, which each say so. Windows reads
/// once and never asks again, which is why this is `unix`.
#[cfg(unix)]
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

/// Screen edges and nothing else, for a session where no X server answers.
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

/// Windows: read windows via EnumWindows, with 500ms refresh for hot-plug.
#[cfg(not(unix))]
pub fn window_source(app: tauri::AppHandle) -> (impl WindowSource, DisplayCache) {
    let cache = DisplayCache(Arc::new(Mutex::new(read_displays(&app))));
    let refreshed = Arc::new(Mutex::new(Instant::now()));

    let source = windows::WindowsWindowSource::new({
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

    (source, cache)
}

/// Windows needs the time check that unix lanes already use.
#[cfg(not(unix))]
fn due(refreshed: &Mutex<Instant>) -> bool {
    let Ok(mut refreshed) = refreshed.lock() else {
        return false;
    };
    if refreshed.elapsed() < Duration::from_millis(500) {
        return false;
    }
    *refreshed = Instant::now();
    true
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
    /// `buttons_down`; without them a click on the sprite is silent.
    #[test]
    fn overlay_primary_is_enough_for_a_press() {
        set_overlay_primary(false);
        set_overlay_primary(true);
        assert!(
            buttons_down().primary,
            "a click the overlay felt must count as the button down"
        );
        set_overlay_primary(false);
        // The session poll may still be true if a real button is held during
        // the test; only the overlay half is under this test's control.
    }

    /// A click can begin and end between two polls. The level alone reads
    /// false at both, so no Poke is ever minted; the edge keeps the down until
    /// it has been read once, so the press is seen exactly once and then gone.
    /// #182.
    #[test]
    fn a_click_shorter_than_one_tick_is_still_seen_once() {
        let button = Witness::new();
        button.report(true);
        button.report(false);
        assert!(
            button.take(),
            "a press that came and went before anyone looked is still a press"
        );
        assert!(!button.take(), "and only once: the read consumes the edge");
    }

    /// The edge is for the missed down, not a second gesture. A real hold reads
    /// true on every tick from the level, and letting go leaves nothing behind
    /// that a later tick could mistake for another press — which is what would
    /// turn every drag into a drag and then a Poke.
    #[test]
    fn a_held_button_reads_true_every_tick_and_nothing_after_release() {
        let button = Witness::new();
        button.report(true);
        assert!(button.take());
        assert!(button.take(), "still held: the level carries it");
        button.report(false);
        assert!(!button.take(), "released");
        assert!(!button.take(), "and no phantom press follows the release");
    }

    /// A pointerup the webview never delivered would leave the level set.
    /// Once the overlay is passing clicks through it cannot still be holding
    /// a press, so both bits must drop — otherwise `buttons_down`
    /// stays true and the sprite glues to a button nobody is pressing. One
    /// `Witness` serves both buttons, so one test covers both.
    #[test]
    fn passing_clicks_through_forgets_a_press_the_overlay_never_released() {
        let button = Witness::new();
        button.report(true);
        button.forget();
        assert!(
            !button.take(),
            "click-through means the overlay is not a witness, so a lost pointerup must not keep the latch"
        );
    }

    /// Memory has no file until the Director has something to remember, so
    /// the opener is handed a path that does not exist yet. Spawning the real
    /// opener is not something a test does; the step that has to happen before
    /// it is.
    #[test]
    fn the_file_is_there_before_the_opener_is() {
        let dir = std::env::temp_dir().join(format!(
            "ai-buddy-open-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("memory.md");

        ensure_file(&path).expect("a missing Memory Manifest is created, not an error");
        assert_eq!(fs::read_to_string(&path).unwrap(), "");

        fs::write(&path, "remembered").unwrap();
        ensure_file(&path).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "remembered",
            "opening Memory must not blank it"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// A path with a space in it is the case that breaks on Unix openers if
    /// the path is split into multiple arguments. The path is the last
    /// argument; what precedes it is what each arm has to get right.
    #[cfg(unix)]
    #[test]
    fn the_opener_is_handed_the_whole_path() {
        let path = Path::new("/tmp/ai buddy/memory.md");
        let command = opener(path);

        assert_eq!(command.get_args().last(), Some(path.as_os_str()));
        #[cfg(target_os = "macos")]
        assert_eq!(command.get_program(), "open");
        #[cfg(all(unix, not(target_os = "macos")))]
        assert_eq!(command.get_program(), "xdg-open");
    }

    /// `#255`: `&` and `%` are `cmd` metacharacters. ShellExecuteW must see
    /// the literal path — including a space — as one wide string, not as
    /// shell text. Encoding is the seam a unit test can observe without
    /// launching a viewer.
    #[cfg(not(unix))]
    #[test]
    fn windows_opener_keeps_ampersand_percent_and_space() {
        let path = Path::new(r"C:\Users\a & b\100%\memory.md");
        let wide = shell_execute_file_wide(path);
        assert_eq!(
            wide.last().copied(),
            Some(0),
            "ShellExecuteW needs a trailing NUL"
        );
        let decoded = String::from_utf16(&wide[..wide.len() - 1]).expect("path is UTF-16");
        assert_eq!(
            decoded,
            path.to_str().expect("test path is UTF-8"),
            "the wide argument must be the path as written, not a cmd-escaped form"
        );
    }

    /// Live check that ShellExecuteW accepts a path with all three
    /// metacharacters. Opens the default `.md` handler briefly; the
    /// acceptance criterion is a success return, not which app appears.
    #[cfg(not(unix))]
    #[test]
    fn open_path_succeeds_for_metacharacter_path() {
        let root = std::env::temp_dir().join(format!(
            "ai-buddy-open-meta-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = root.join("a & b").join("100%").join("memory.md");
        let _ = fs::remove_dir_all(&root);
        open_path(&path).expect("ShellExecuteW must open a path that holds &, %, and a space");
        assert!(path.is_file(), "ensure_file still creates Memory first");
        let _ = fs::remove_dir_all(&root);
    }

    /// A right-click on the overlay is the same miss as a left-click. Without
    /// this witness the webview's Inspect menu is the only thing that hears it.
    #[test]
    fn overlay_secondary_is_enough_for_a_press() {
        set_overlay_secondary(false);
        set_overlay_secondary(true);
        assert!(
            buttons_down().secondary,
            "a right-click the overlay felt must count as the button down"
        );
        set_overlay_secondary(false);
    }
}
