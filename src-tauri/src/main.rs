//! ai-buddy's overlay shell.
//!
//! One transparent, always-on-top window per display renders the Character.
//! Click-through on macOS is per-window rather than per-pixel, so a screen-sized
//! transparent window would swallow every click. The shell therefore tracks the
//! cursor and toggles ignore-mouse-events by hit-testing the sprite's alpha,
//! which is what makes the overlay feel like a sprite on the desktop instead of
//! a sheet of glass over it.
//!
//! It also owns the frame loop, which is the only thing that can: the Engine is
//! pure and cannot read a clock, and `WindowSource` reports geometry and nothing
//! else. The loop reads the wall clock and the cursor, asks
//! `SnapshotAssembler` for a `WorldSnapshot`, ticks the Engine, and hands the
//! resulting `Frame` to the webview and to the hit-test.
//!
//! Waking the Director is the loop's too, and for the same reason: a timer
//! is a clock. Static may wake often. A session wake is reactive or backed
//! off (ADR-0008). What it proposes is `director`'s; when it is asked is here.

// The native settings window is macOS and Linux only. On Windows
// platform::show_settings is a stub, so the form these three modules build for
// it, and the secret writes its rows perform, are unreachable there by design
// rather than rotted. `docs/SPEC.md` puts Windows out of v1. #247.
//
// ponytail: module-wide, where only part of each module is actually dead on
// Windows. `settings` in particular is about half live, since main.rs uses
// Settings, parse_hotkey, toggle_away and settings_path on every platform. The
// ceiling is that dead code added inside these three goes unwarned on Windows;
// it costs nothing today because macOS and Linux still lint every item. Narrow
// it to `mod form` and the view types when a Windows-only item first lands.
#[cfg_attr(not(unix), allow(dead_code))]
mod consent;
mod env_util;
mod frame_loop;
mod menu;
mod model;
mod package;
mod platform;
#[cfg_attr(not(unix), allow(dead_code))] // see the note on `consent`
mod secrets;
#[cfg_attr(not(unix), allow(dead_code))] // see the note on `consent`
mod settings;
mod tray;

use frame_loop::run_frame_loop;

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy_core::character::Character;
use ai_buddy_core::director::{Context, Happened, ModelDirector, Pace, Seeded, StaticDirector};
use ai_buddy_core::engine::{Point, State, Verb};
use ai_buddy_core::input::Pointer;
use ai_buddy_core::memory::{self, MemoryManifest};
use ai_buddy_core::overlay::SpriteRect;
use ai_buddy_core::roster::{self, InstanceId, InstanceSpec, Roster};
use ai_buddy_core::snapshot::starting_position;
use ai_buddy_core::visibility::HideRules;
use ai_buddy_core::window_source::{Rect, WindowSource};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use secrets::{KeyringStore, SecretStore};
use serde::Serialize;
use settings::{InstanceRow, Settings, SettingsOp, SettingsSession};
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Where the shipped Character Packages sit inside the app's resources. Kept in
/// step with `bundle.resources` in `tauri.conf.json`.
const BUNDLED_CHARACTERS: &str = "characters";

/// One turn of the frame loop: roughly 60Hz. The desktop is read at 10Hz
/// while the sprite is still, and at this rate only while it is riding.
///
/// It is a poll rather than an event stream for two reasons. A click-through
/// window receives no mouse events at all, so the webview cannot tell us when
/// the cursor returns — something outside the window has to ask where it is.
/// And the Engine advances on elapsed time, so something has to advance it.
const ENGINE_TICK: Duration = Duration::from_millis(16);

/// How often the Free tier is read.
///
/// Far less often than the frame loop turns: the answers change at human speed,
/// and each read is two calls into AppKit and CoreGraphics that the sprite's
/// physics have no use for.
const SENSE_INTERVAL: Duration = Duration::from_secs(1);

/// The label of the overlay covering the display at `index`.
///
/// One window per display, so the index is both the name and the way the frame
/// loop finds the overlay belonging to a display. `capabilities/overlay.json`
/// grants the same permissions to every `overlay-*`.
fn overlay_label(index: usize) -> String {
    format!("overlay-{index}")
}

/// The event carrying each `Frame` to the webview.
const FRAME_EVENT: &str = "frame";

/// Director config and the last Character Prompt, for the frame loop.
struct DirectorRun {
    config: model::DirectorConfig,
    settings: model::DirectorSettings,
    inspect: Arc<Mutex<model::DirectorInspect>>,
}

/// Where the sprite was last drawn, and what it was drawn as.
///
/// Kept for one tick so the hit-test can ask about the sprite the user is
/// looking at rather than the one this tick is about to produce.
struct Drawn {
    rect: SpriteRect,
    animation: &'static str,
    animation_ms: u32,
    /// Whether the art was drawn mirrored, so the hit-test feels the same
    /// pixels the user saw — this tick's facing may already differ.
    mirrored: bool,
}

/// Everything one Instance keeps between ticks that belongs to the Shell rather
/// than to its Engine.
///
/// The Roster owns the Engines, which is the whole of what an Instance is in the
/// pure core. None of this can live there: a Director that posts over the
/// network, a pointer gesture measured against art, and the last rectangle the
/// hit-test used are all things the core has no window server for.
///
/// Every field is here because sharing it between Instances would be visible.
/// One Director for two buddies would have them move in lockstep; one `Pointer`
/// would have a double-click on one count towards a Summon on the other; one
/// `Drawn` would hit-test both against whichever drew last.
struct InstanceState {
    /// Which Instance in the Roster this belongs to.
    id: InstanceId,
    /// The Character this Instance runs, shared with every other Instance
    /// running the same one.
    character: Arc<Character>,
    director: StaticDirector,
    model: Option<Arc<ModelDirector<model::Endpoint>>>,
    pending: model::InFlight,
    in_flight: Option<Context>,
    recent: Vec<String>,
    pace: Pace,
    since_wake: Duration,
    since_state: Duration,
    since_ambient: Duration,
    previous_idle: Duration,
    last_state: Option<State>,
    addressed: bool,
    happened: Happened,
    pointer: Pointer,
    /// The last line spoken and which overlay showed it, so a crossing
    /// carries it (#178). See `carry_line`.
    spoken: Option<Spoken>,
    drawn_last: Option<Drawn>,
    /// This tick's verbs, decided before any Instance is ticked. Held on the
    /// Instance because `press_target` has to see every hit-test before any
    /// pointer is told whether the press was its own.
    verbs: Vec<Verb>,
    /// The menu this Instance has open, and `None` when it has none. While it is
    /// `Some`, the frame loop re-injects `Verb::Menu` every tick, which is what
    /// holds the Instance still under the popup.
    menu_hold: Option<MenuHold>,
}

/// One open menu, from the frame loop's side.
struct MenuHold {
    /// What the rows of the menu on screen mean.
    ///
    /// Kept rather than looked up again when the click arrives, because the menu
    /// on screen is the menu that was described: a package installed while it is
    /// open must not change what its rows do.
    actions: HashMap<String, menu::MenuAction>,
    /// How long it has been open, against `MENU_HOLD_TIMEOUT`.
    elapsed: Duration,
}

/// What the main thread tells the frame loop about the menu it was asked to pop.
///
/// Two messages rather than one because a menu can close without choosing
/// anything, and that has to end the hold too. Nothing arrives on the event
/// channel when the user presses Escape.
enum MenuSignal {
    /// A row was chosen, by the id the description gave it.
    Chose(String),
    /// The popup is gone, whether or not anything was chosen.
    Closed,
}

/// Both ends of the menu's channel, handed to the frame loop together.
///
/// The sender goes to the main thread with each popup and the receiver is
/// drained every tick. They travel as a pair because the app's menu event hook
/// is registered before the frame loop starts and needs a sender of its own.
struct MenuChannel {
    sender: mpsc::Sender<MenuSignal>,
    receiver: mpsc::Receiver<MenuSignal>,
}

/// Settings plus the live roster the settings window reads.
struct SettingsState {
    settings: Arc<Mutex<Settings>>,
    path: PathBuf,
    memory_path: PathBuf,
    installed: Vec<String>,
    instances: Arc<Mutex<Vec<InstanceRow>>>,
    inspect: Arc<Mutex<model::DirectorInspect>>,
    ops: mpsc::Sender<SettingsOp>,
    rules: Arc<Mutex<HideRules>>,
    secrets: Arc<dyn SecretStore>,
}

/// How long a hold survives without hearing anything before it is dropped.
///
/// A backstop, not a mechanism. `Closed` ends the hold; this only matters if it
/// never comes, and the failure it prevents is the one that cannot be
/// recovered from — an Instance frozen under a menu that is no longer there,
/// for as long as the app runs.
const MENU_HOLD_TIMEOUT: Duration = Duration::from_secs(120);

/// Where one Instance is to be drawn in one overlay, in logical points from
/// that overlay's top-left, and which Animation frame to draw.
#[derive(Clone, Serialize)]
struct SpritePlacement<'a> {
    /// The Instance this sprite belongs to, so the renderer keeps one element
    /// per Instance across ticks rather than redrawing a fresh set. An id that
    /// stops arriving is an Instance that was dismissed, and its element goes.
    id: &'a str,
    /// Which Character's art to draw from. Instances may run different
    /// Characters, and two running the same one name the same art.
    character: &'a str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// The Animation whose art to draw — the one `Character::draw` resolved
    /// (a variant, or an optional Animation's fallback), which is not always
    /// the name the Engine asked with.
    animation: &'a str,
    frame_index: usize,
    /// -1 to mirror the art (heading left), 1 to draw it as authored.
    facing: i8,
    /// A line to speak on this tick only. Dialogue is an event, not a state.
    /// #119: the webview latches it and owns display duration.
    dialogue: Option<String>,
    /// Whether to show the thinking ellipsis. Derived from InFlight state and
    /// addressed. #119: grace and min-hold are in the webview so the Engine
    /// stays tick-pure.
    thinking: bool,
    /// Whether this overlay draws this Instance's bubble (#178, `bubble_owner`).
    bubble: bool,
}

/// One tick's instruction to the renderer: every Instance's sprite, and whether
/// the Character is on screen at all.
///
/// Pushed every tick rather than fetched, so the webview holds no authoritative
/// state — it draws what it was last told and remembers nothing.
///
/// One message carrying every sprite rather than one per Instance, because the
/// list is also the answer to which Instances still exist. Sent separately, a
/// dismissed Instance would simply stop being mentioned, and nothing would say
/// whether its last message was the end or the next one was merely late.
#[derive(Clone, Serialize)]
struct Placement<'a> {
    sprites: Vec<SpritePlacement<'a>>,
    /// Whether the hide rules have the Character on screen, and how long the
    /// change that decided it was given. One answer for every Instance: the
    /// rules are about the desktop, not about a sprite.
    ///
    /// Carried on every frame rather than announced on the tick it changes.
    /// The first tick fires 16ms into setup, before the webview has fetched
    /// its art and begun listening, and Tauri buffers nothing for a listener
    /// that is not there yet — so a Character hidden at launch would be told
    /// to go once, to nobody, and stay on top of the fullscreen application
    /// all session.
    visible: bool,
    fade_ms: u32,
}

/// What one Instance's tick decided to draw, in the space every display shares.
///
/// The step between ticking the Instances and telling the overlays. Every
/// overlay is told about every Instance in its own coordinates, so the
/// placement is worked out once here and turned into each overlay's rectangle
/// below — and the art names are owned rather than borrowed so that this
/// outlives the borrow of the Character it came from.
/// A spoken line, when, and the overlay that was showing it.
struct Spoken {
    line: String,
    at: Instant,
    owner: Option<usize>,
}

/// The longest the renderer keeps a line up — `bubbleDuration`'s clamp in
/// `src/bubble.js`. A line older than this cannot still be showing anywhere,
/// so it is never carried.
const CARRY_WINDOW: Duration = Duration::from_secs(8);

/// The dialogue this tick's placement carries: the line the Engine just said,
/// or the last one re-pulsed to a new owner.
///
/// Dialogue is a one-tick pulse, and only the overlay that owns the bubble on
/// that tick latches it. A sprite that crosses a seam mid-line would otherwise
/// lose the line: the old owner hides on the change, and the new one never saw
/// the pulse (#178). So an owner change within the reading window says the
/// line again to the new owner. Its reading time restarts there, which is
/// the cheaper of the two honest answers — the renderer owns that clock.
fn carry_line(
    spoken: &mut Option<Spoken>,
    said: Option<&str>,
    owner: Option<usize>,
    now: Instant,
) -> Option<String> {
    if let Some(line) = said {
        *spoken = Some(Spoken {
            line: line.to_string(),
            at: now,
            owner,
        });
        return Some(line.to_string());
    }
    let carried = spoken.as_mut()?;
    if carried.owner == owner || now.duration_since(carried.at) > CARRY_WINDOW {
        return None;
    }
    carried.owner = owner;
    Some(carried.line.clone())
}

struct Placed {
    id: InstanceId,
    character: String,
    sprite: SpriteRect,
    width: i32,
    height: i32,
    animation: String,
    frame_index: usize,
    facing: i8,
    dialogue: Option<String>,
    thinking: bool,
    /// The overlay that draws the bubble, decided once from the feet
    /// (#178, `bubble_owner`); `None` while the feet are on no display.
    owner: Option<usize>,
    #[allow(dead_code)]
    mask: ai_buddy_core::overlay::AlphaMask,
}

/// Every Animation's frames as `data:` URLs, in play order, keyed by the
/// Animation's name.
///
/// URLs rather than file paths because a Character Package lives outside the
/// front end's own directory — in the user's Application Support, or wherever
/// they put it — so there is no URL the webview could fetch a frame from.
/// Handing over the bytes avoids granting the webview a filesystem scope for
/// the sake of drawing a sprite.
///
/// The webview picks a frame out of each list by the index the frame loop
/// sends, so the order here has to be the play order `Character::draw` indexes
/// — both walk `Animation::frames` as declared. Indexing `art` cannot miss: a
/// validated Character carries art for every frame its Animations name.
fn art_urls(character: &Character) -> BTreeMap<String, Vec<String>> {
    // A frame two Animations share is encoded once and named twice.
    let urls: BTreeMap<&String, String> = character
        .art
        .iter()
        .map(|(frame, art)| {
            let url = format!("data:image/png;base64,{}", STANDARD.encode(&art.png));
            (frame, url)
        })
        .collect();

    character
        .animations
        .iter()
        .map(|(name, animation)| {
            let frames = animation.frames.iter().map(|frame| urls[frame].clone());
            (name.clone(), frames.collect())
        })
        .collect()
}

/// What the webview needs of one Character: the art as `data:` URLs, and
/// whether to smooth it when scaling (the Character Manifest's `render_mode`).
#[derive(Clone, serde::Serialize)]
struct CharacterArt {
    art: BTreeMap<String, Vec<String>>,
    smooth: bool,
}

/// Every Character on screen, by name, as Tauri managed state.
///
/// Keyed by Character rather than by Instance, which is what keeps the art one
/// copy: two Instances of one Character are the common case #13 exists for, and
/// encoding a sprite sheet twice to draw the same pet twice would double the
/// heaviest thing the app holds.
///
/// A struct rather than a bare map so managed state, keyed by type, cannot
/// collide with another map of the same shape.
#[derive(Clone, serde::Serialize)]
struct ArtUrls {
    characters: BTreeMap<String, CharacterArt>,
}

/// The art of every Character on screen, fetched once by the webview when it
/// loads.
///
/// A command rather than an event: an event emitted during setup would race the
/// webview's own listener, and the art does not change while the app runs —
/// which is also why the Instances that may run are settled at launch. Spawning
/// one afterwards is #18's, and it will have to hand the webview art this
/// command has already answered without.
#[tauri::command]
fn character(art: tauri::State<'_, ArtUrls>) -> ArtUrls {
    art.inner().clone()
}

/// Settings is native Shell furniture (AppKit on macOS, GTK 3 on Linux).
/// SPEC gives the webview to the sprite and chat, so this is opened on the
/// toolkit main thread where the native objects live.
fn show_settings(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<SettingsState>() else {
        eprintln!("settings: opened before the shell was ready");
        return;
    };
    let session = SettingsSession {
        settings: Arc::clone(&state.settings),
        path: state.path.clone(),
        memory_path: state.memory_path.clone(),
        rules: Arc::clone(&state.rules),
        inspect: Arc::clone(&state.inspect),
        instances: Arc::clone(&state.instances),
        installed: state.installed.clone(),
        ops: state.ops.clone(),
        app: app.clone(),
        on_rebind: bind_hide_hotkey,
        secrets: Arc::clone(&state.secrets),
        key_cache: Mutex::new(None),
    };

    // On Linux, if the MainContext is already owned (menu/tray callback runs
    // on the GTK main thread), use idle_add_local_once to defer window creation
    // until after the current event completes. A sync wait or inline create
    // deadlocks. If not the owner, invoke posts without waiting.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let ctx = gtk::glib::MainContext::default();
        if ctx.is_owner() {
            gtk::glib::idle_add_local_once(move || {
                platform::show_settings(session);
            });
        } else {
            ctx.invoke(move || {
                platform::show_settings(session);
            });
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    if let Err(why) = app.run_on_main_thread(move || platform::show_settings(session)) {
        eprintln!("settings: {why}");
    }
}

fn persist_settings(settings: &Settings, path: &std::path::Path) {
    if let Err(why) = settings.save(path) {
        eprintln!("settings: {why}");
    }
}

fn remember_instances(roster: &Roster, settings: &Arc<Mutex<Settings>>, path: &std::path::Path) {
    if let Ok(mut settings) = settings.lock() {
        settings.instances = roster
            .list()
            .into_iter()
            .map(|(id, name)| InstanceSpec {
                character: roster
                    .get(&id)
                    .map(|instance| instance.character_name().to_string())
                    .unwrap_or_default(),
                name,
            })
            .collect();
        persist_settings(&settings, path);
    }
}

/// The overlay heard the primary button. The frame loop polls a session
/// query that can miss a click on this window; this is the other witness.
#[tauri::command]
fn overlay_primary(down: bool) {
    platform::set_overlay_primary(down);
}

/// Same witness for the right button. Without it a right-click on the sprite
/// is swallowed by the webview and the session poll never sees a Menu.
#[tauri::command]
fn overlay_secondary(down: bool) {
    platform::set_overlay_secondary(down);
}

/// Put the overlay over one display, covering it exactly.
///
/// Sized before it is moved. Growing a window anchors its bottom-left corner,
/// so a window resized after it is placed pushes its own top edge off the
/// display it was just put on.
fn cover_display(window: &tauri::WebviewWindow, display: Rect) -> Result<(), tauri::Error> {
    window.set_size(LogicalSize::new(display.width, display.height))?;
    window.set_position(LogicalPosition::new(display.x, display.y))
}

/// Build one overlay, configure it, and put it over its display.
///
/// The only place an overlay is made. Click-through, window level, Spaces
/// membership and hide rules have to be identical on every overlay, and a
/// second window is a second place for them to disagree; one function called
/// once per display is what keeps them one set of rules instead of two.
///
/// Main thread only: it builds a window and calls AppKit.
fn build_overlay(
    app: &tauri::AppHandle,
    label: &str,
    display: Rect,
) -> Result<(), Box<dyn std::error::Error>> {
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title("ai-buddy")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .accept_first_mouse(true)
        .focused(false)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .build()?;

    // On Linux/GTK, set_ignore_cursor_events queues a tao WindowRequest that
    // unwraps the GdkWindow, which is None until the widget is realized (needs
    // the event loop, not just show()). The frame loop sets ignore-cursor on
    // the first frame once the window exists. On macOS, NSWindow exists while
    // hidden, so the call is safe and establishes the initial state.
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    window.set_ignore_cursor_events(true)?;

    cover_display(&window, display)?;
    // Show the window first so GTK realizes it and creates the native handle.
    // Linux (GTK) has no GdkWindow until the widget is realized; macOS NSWindow
    // exists while hidden.
    window.show()?;

    // Linux: configure_overlay may fail if the GTK widget is not yet realized.
    // The frame loop retries on the main thread, so a failure here is not fatal.
    // macOS: NSWindow is always ready, so failure is a real error.
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Err(why) = platform::configure_overlay(&window) {
        eprintln!("overlay: {label} EWMH config deferred: {why}");
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    platform::configure_overlay(&window)?;

    eprintln!(
        "overlay: {label} covers {:.0}x{:.0} at ({:.0},{:.0})",
        display.width, display.height, display.x, display.y,
    );

    Ok(())
}

/// One overlay per display, each covering that display.
///
/// This is what keeps a Character on a seam whole: both overlays draw it, each
/// clipping its own half, and the halves meet. One window cannot do it, because
/// macOS gives each display its own Space and draws a window spanning two of
/// them on only one — a window sized to the display union is invisible
/// everywhere but the display it happens to belong to.
///
/// Idempotent, because the desktop changes while the app runs: a display that
/// already has its overlay keeps it and is only re-covered, which is what a
/// display that moved or changed resolution needs. Overlays past the end of the
/// list belong to displays that have been unplugged.
///
/// Every display is attempted even after one fails, and the failures are
/// returned together. Stopping at the first would leave every display after it
/// without an overlay, which is a worse desktop than the one bad display.
///
/// Main thread only; see `build_overlay`.
fn place_overlays(app: &tauri::AppHandle, displays: &[Rect]) -> Result<(), String> {
    let mut failed = Vec::new();

    for (index, display) in displays.iter().enumerate() {
        let label = overlay_label(index);
        let placed = match app.get_webview_window(&label) {
            Some(window) => cover_display(&window, *display).map_err(|why| why.to_string()),
            None => build_overlay(app, &label, *display).map_err(|why| why.to_string()),
        };
        if let Err(why) = placed {
            failed.push(format!("{label}: {why}"));
        }
    }

    // Labels are handed out in order, so the first missing one ends the set.
    for index in displays.len().. {
        let label = overlay_label(index);
        let Some(window) = app.get_webview_window(&label) else {
            break;
        };
        eprintln!("overlay: {label} has no display left to cover");
        if let Err(why) = window.close() {
            failed.push(format!("{label}: {why}"));
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

/// Register the hotkey that hides and shows the Character.
///
/// Three modifiers, because a global shortcut is taken from every application
/// on the machine and B alone belongs to most of them. The binding is the
/// settings string, rebound when that string changes.
///
/// A hotkey another application already holds is reported and let go. Losing it
/// costs the user one way to hide the Character, which is not worth losing the
/// Character over.
fn shortcut_from_spec(spec: &str) -> Option<Shortcut> {
    let parsed = settings::parse_hotkey(spec)
        .or_else(|| settings::parse_hotkey(settings::DEFAULT_HIDE_HOTKEY))?;
    let code: Code = settings::key_code_name(parsed.key)?.parse().ok()?;
    let mut modifiers = Modifiers::empty();
    if parsed.control {
        modifiers |= Modifiers::CONTROL;
    }
    if parsed.option {
        modifiers |= Modifiers::ALT;
    }
    if parsed.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if parsed.command {
        modifiers |= Modifiers::SUPER;
    }
    Some(Shortcut::new(Some(modifiers), code))
}

fn install_hide_hotkey(
    app: &tauri::AppHandle,
    rules: Arc<Mutex<HideRules>>,
    settings: Arc<Mutex<Settings>>,
    settings_path: PathBuf,
) -> bool {
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |_app, _shortcut, event| {
            // Pressed only. The handler is called again on release, and a
            // toggle that ran twice would hand the Character back before the
            // user had let go of the key.
            if event.state() == ShortcutState::Pressed {
                if let (Ok(mut rules), Ok(mut settings)) = (rules.lock(), settings.lock()) {
                    settings::toggle_away(&mut rules, &mut settings);
                    persist_settings(&settings, &settings_path);
                }
            }
        })
        .build();
    if let Err(why) = app.plugin(plugin) {
        eprintln!("hotkey: unavailable, so the Character cannot be hidden by hand: {why}");
        false
    } else {
        true
    }
}

fn bind_hide_hotkey(app: &tauri::AppHandle, spec: &str) {
    if app
        .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
        .is_none()
    {
        return;
    }
    let Some(shortcut) = shortcut_from_spec(spec) else {
        eprintln!("hotkey: {spec} could not be parsed");
        return;
    };
    let gs = app.global_shortcut();
    if let Err(why) = gs.unregister_all() {
        eprintln!("hotkey: could not drop the previous binding: {why}");
    }
    if let Err(why) = gs.register(shortcut) {
        eprintln!(
            "hotkey: {spec} is unavailable, so the Character cannot be hidden by hand: {why}"
        );
    }
}

fn check_for_update(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;
        let updater = match app.updater() {
            Ok(updater) => updater,
            Err(why) => {
                eprintln!("updater: {why}");
                return;
            }
        };
        match updater.check().await {
            Ok(Some(update)) => {
                eprintln!("updater: {} available, downloading", update.version);
                if let Err(why) = update.download_and_install(|_, _| {}, || {}).await {
                    eprintln!("updater: {why}");
                }
            }
            Ok(None) => eprintln!("updater: up to date"),
            Err(why) => eprintln!("updater: {why}"),
        }
    });
}

// One over the clippy cap. Settings, hide rules, and the Director each have
// to hear the same click, and folding them would mix persist with proposal.
#[allow(clippy::too_many_arguments)]
fn apply_menu_action(
    action: menu::MenuAction,
    roster: &mut Roster,
    lives: &mut Vec<InstanceState>,
    instance_id: &InstanceId,
    rules: &Arc<Mutex<HideRules>>,
    settings: &Arc<Mutex<Settings>>,
    settings_path: &std::path::Path,
    characters: &BTreeMap<String, Arc<Character>>,
    config: &mut model::DirectorConfig,
    director: &model::DirectorSettings,
    inspect: &Arc<Mutex<model::DirectorInspect>>,
    app: &tauri::AppHandle,
) {
    match action {
        menu::MenuAction::SwitchCharacter(name) => {
            if let Some(character) = characters.get(&name).cloned() {
                switch_instance(roster, lives, instance_id, character, config, director);
                if let Ok(mut settings) = settings.lock() {
                    settings.character = name.clone();
                    persist_settings(&settings, settings_path);
                }
                eprintln!("menu: switching to {name}");
            } else {
                eprintln!("menu: no Character named {name}");
            }
        }
        menu::MenuAction::SpawnInstance => {
            let character_name = settings
                .lock()
                .ok()
                .map(|s| s.character.clone())
                .filter(|name| !name.is_empty())
                .or_else(|| lives.first().map(|live| live.character.name.clone()));
            if let Some(name) = character_name {
                spawn_live(
                    roster,
                    lives,
                    characters,
                    &name,
                    name.clone(),
                    config,
                    director,
                );
            }
        }
        menu::MenuAction::ToggleDirector => {
            if let Ok(mut settings) = settings.lock() {
                settings.director_enabled = !settings.director_enabled;
                config.enabled = settings.director_enabled && config.configured;
                if let Ok(mut inspect) = inspect.lock() {
                    inspect.enabled = config.enabled;
                }
                persist_settings(&settings, settings_path);
                eprintln!(
                    "menu: Director {}",
                    if settings.director_enabled {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
        }
        menu::MenuAction::ToggleDnd => {
            if let Some(instance) = roster.get_mut(instance_id) {
                let new_state = !instance.do_not_disturb();
                instance.set_do_not_disturb(new_state);
                if let Ok(mut settings) = settings.lock() {
                    settings.do_not_disturb = new_state;
                    persist_settings(&settings, settings_path);
                }
                eprintln!("menu: DND {}", if new_state { "on" } else { "off" });
            }
        }
        menu::MenuAction::Hide => {
            if let Ok(mut r) = rules.lock() {
                if let Ok(mut settings) = settings.lock() {
                    settings::toggle_away(&mut r, &mut settings);
                    persist_settings(&settings, settings_path);
                }
                eprintln!("menu: {}", if r.is_away() { "away" } else { "back" });
            }
        }
        menu::MenuAction::ToggleFullscreenHide => {
            if let Ok(mut r) = rules.lock() {
                let next = !r.hide_in_fullscreen();
                r.set_hide_in_fullscreen(next);
                if let Ok(mut settings) = settings.lock() {
                    settings.hide_in_fullscreen = r.hide_in_fullscreen();
                    persist_settings(&settings, settings_path);
                }
            }
        }
        menu::MenuAction::OpenMemory => {
            let _ = platform::open_path(&memory::shared_path());
        }
        menu::MenuAction::OpenSettings => show_settings(app),
        menu::MenuAction::Quit => quit_now(),
    }
}

/// Leave without AppKit's `terminate:`.
///
/// `PredefinedMenuItem::quit` calls `[NSApp terminate:]` from inside the tray
/// menu's tracking run loop. That teardown deadlocks against the overlay
/// webviews the frame loop is still drawing into, and a hung full-display
/// overlay is a desktop the user cannot click. `process::exit` skips that path;
/// the window server drops the overlays with the process.
fn quit_now() -> ! {
    eprintln!("quit");
    std::process::exit(0);
}

fn switch_instance(
    roster: &mut Roster,
    lives: &mut [InstanceState],
    instance_id: &InstanceId,
    character: Arc<Character>,
    config: &model::DirectorConfig,
    settings: &model::DirectorSettings,
) {
    roster.retarget(instance_id, &character);
    if let Some(live) = lives.iter_mut().find(|live| live.id == *instance_id) {
        live.character = character;
        live.director = StaticDirector::new(live.character.behaviors.clone(), 0);
        live.pace = Pace::with_growth(
            config.ambient_first,
            live.character.model_base,
            live.character.model_power,
        );
        // The old session is the previous Character's. A Wake still on the
        // wire would propose as them; drop it and ask for this opening turn.
        model::retarget_model(
            &mut live.pending,
            &mut live.in_flight,
            &mut live.model,
            live.character.behaviors.keys().cloned(),
            settings,
            config.configured,
        );
        live.recent.clear();
        live.happened = Happened::Ambient;
        live.addressed = true;
    }
}

fn spawn_live(
    roster: &mut Roster,
    lives: &mut Vec<InstanceState>,
    characters: &BTreeMap<String, Arc<Character>>,
    character_name: &str,
    instance_name: String,
    config: &model::DirectorConfig,
    settings: &model::DirectorSettings,
) {
    let Some(character) = characters.get(character_name).cloned() else {
        eprintln!("menu: no Character named {character_name}");
        return;
    };
    let start = lives
        .last()
        .and_then(|live| live.drawn_last.as_ref())
        .map(|drawn| Point {
            x: f64::from(drawn.rect.x) + sprite_width(&character) + 16.0,
            y: f64::from(drawn.rect.y),
        })
        .unwrap_or(Point { x: 80.0, y: 80.0 });
    let name = if instance_name.is_empty() {
        character.name.clone()
    } else {
        instance_name
    };
    let id = roster.spawn(&character, name, start);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    lives.push(InstanceState {
        id,
        director: StaticDirector::new(character.behaviors.clone(), seed),
        model: config.configured.then(|| {
            Arc::new(ModelDirector::new(
                model::endpoint_from(settings).expect("configured means a Completer exists"),
                character.behaviors.keys().cloned(),
            ))
        }),
        pending: model::InFlight::new(),
        in_flight: None,
        recent: Vec::new(),
        pace: Pace::with_growth(
            config.ambient_first,
            character.model_base,
            character.model_power,
        ),
        since_wake: Duration::ZERO,
        since_state: Duration::ZERO,
        since_ambient: Duration::ZERO,
        previous_idle: Duration::MAX,
        last_state: None,
        addressed: false,
        happened: Happened::Ambient,
        pointer: Pointer::default(),
        spoken: None,
        drawn_last: None,
        verbs: Vec::new(),
        menu_hold: None,
        character,
    });
}

/// The menu bar icon. Held so a toggle on the frame-loop thread can rebuild
/// the menu on the main thread, where the native objects live.
struct TrayHandle(Mutex<Option<tauri::tray::TrayIcon>>);

struct FrameExtras {
    settings: Arc<Mutex<Settings>>,
    settings_path: PathBuf,
    characters: BTreeMap<String, Arc<Character>>,
    instances: Arc<Mutex<Vec<InstanceRow>>>,
    ops: mpsc::Receiver<SettingsOp>,
}

fn publish_instances(roster: &Roster, dest: &Arc<Mutex<Vec<InstanceRow>>>) {
    if let Ok(mut rows) = dest.lock() {
        *rows = roster
            .list()
            .into_iter()
            .map(|(id, name)| {
                let character = roster
                    .get(&id)
                    .map(|instance| instance.character_name().to_string())
                    .unwrap_or_default();
                InstanceRow {
                    id,
                    name,
                    character,
                }
            })
            .collect();
    }
}

fn describe_menu(
    installed: &[String],
    current: &str,
    roster: &Roster,
    instance_id: &str,
    settings: &Settings,
    rules: &HideRules,
) -> menu::MenuDescription {
    let instances = roster.list();
    menu::describe(menu::MenuSnapshot {
        installed,
        current_character: current,
        instances: &instances,
        director_enabled: settings.director_enabled,
        do_not_disturb: roster
            .get(instance_id)
            .map(|instance| instance.do_not_disturb())
            .unwrap_or(false),
        hidden: rules.is_away(),
        hide_in_fullscreen: rules.hide_in_fullscreen(),
        hide_hotkey: &settings.hide_hotkey,
    })
}

/// The environment variable naming the Instances to run.
///
/// An environment variable rather than a flag because it is how ai-buddy is
/// already configured — `AI_BUDDY_CHARACTER` picks the Character, and the trace
/// flags turn the logs on — and a second mechanism for the same kind of answer
/// is a second place to look it up.
const INSTANCES_VAR: &str = "AI_BUDDY_INSTANCES";

/// Which Instances the launch configuration asks for.
///
/// The environment wins when a developer set it. Otherwise settings. Empty is
/// still first-run: `load_instances` turns it into the one buddy the app has
/// always run.
fn requested_instances(settings: &Settings) -> Result<Vec<InstanceSpec>, String> {
    match std::env::var(INSTANCES_VAR) {
        Ok(raw) => roster::parse_specs(&raw),
        Err(_) if !settings.instances.is_empty() => Ok(settings.instances.clone()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Load the Character each requested Instance names, sharing one load between
/// Instances that name the same one.
///
/// Asking for no Instances runs the one ai-buddy has always run: the Character
/// `AI_BUDDY_CHARACTER` names, or the default, called after itself. That has to
/// keep working, or every existing way of starting the app would start something
/// different.
///
/// The Character comes back beside the spec that asked for it, and an Instance
/// with no name of its own takes the Character's — which is what `bmo` alone
/// means.
fn load_all_characters(
    app: &tauri::AppHandle,
) -> (
    BTreeMap<String, CharacterArt>,
    BTreeMap<String, Arc<Character>>,
) {
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(BUNDLED_CHARACTERS));
    let search_paths = package::search_paths(bundled);
    let mut art = BTreeMap::new();
    let mut cache = BTreeMap::new();
    for path in package::installed(&search_paths) {
        let files = match package::read(&path) {
            Ok(files) => files,
            Err(_) => continue,
        };
        if let Ok(character) = ai_buddy_core::character::load(&files) {
            art.insert(
                character.name.clone(),
                CharacterArt {
                    art: art_urls(&character),
                    smooth: character.smooth,
                },
            );
            cache.insert(character.name.clone(), Arc::new(character));
        }
    }
    (art, cache)
}

fn load_instances(
    app: &tauri::AppHandle,
    wanted: &[InstanceSpec],
    settings: &Settings,
) -> Result<Vec<(InstanceSpec, Arc<Character>)>, String> {
    if wanted.is_empty() {
        let wanted = std::env::var_os(package::CHARACTER_VAR).or_else(|| {
            (!settings.character.is_empty()).then(|| std::ffi::OsString::from(&settings.character))
        });
        let character = Arc::new(load_named(app, wanted)?);
        let name = character.name.clone();
        return Ok(vec![(
            InstanceSpec {
                character: character.name.clone(),
                name,
            },
            character,
        )]);
    }

    let mut loaded: BTreeMap<String, Arc<Character>> = BTreeMap::new();
    let mut instances = Vec::with_capacity(wanted.len());

    for spec in wanted {
        let character = match loaded.get(&spec.character) {
            Some(character) => Arc::clone(character),
            None => {
                let character = Arc::new(load_named(
                    app,
                    Some(std::ffi::OsString::from(&spec.character)),
                )?);
                loaded.insert(spec.character.clone(), Arc::clone(&character));
                character
            }
        };

        let name = if spec.name.is_empty() {
            character.name.clone()
        } else {
            spec.name.clone()
        };
        instances.push((
            InstanceSpec {
                character: spec.character.clone(),
                name,
            },
            character,
        ));
    }

    Ok(instances)
}

/// Spawn every requested Instance into a Roster, and build the Shell state each
/// one keeps beside its Engine.
///
/// Memory is one file for every Instance, which is what makes a second buddy
/// already know the user: `Roster` holds it behind an `Arc` and hands the same
/// one to each.
fn spawn_instances(
    loaded: &[(InstanceSpec, Arc<Character>)],
    start: Point,
    config: &model::DirectorConfig,
    settings: &model::DirectorSettings,
) -> (Roster, Vec<InstanceState>) {
    let mut roster = Roster::new(MemoryManifest::new(memory::shared_path()));
    let mut lives = Vec::with_capacity(loaded.len());

    // The wall clock, so that two runs are not the same afternoon — the one
    // thing the Engine's own purity forbids it to do. Mixed with the Instance's
    // place in the list below, because Instances built in the same nanosecond
    // would otherwise share a seed and make the same choices for as long as they
    // both lived, which is the lockstep #13 asks not to have.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);

    // Where each Instance's wake clock starts. Drawn from the same launch seed
    // rather than from the clock again: `as_nanos` read three times in a row
    // differs in its low bits only, and a phase taken from that would put every
    // buddy within a millisecond of the others.
    let mut phases = Seeded::new(seed);

    let widths: Vec<f64> = loaded
        .iter()
        .map(|(_, character)| sprite_width(character))
        .collect();
    let positions = starting_positions(start, &widths);

    for (index, (spec, character)) in loaded.iter().enumerate() {
        let id = roster.spawn(character, spec.name.clone(), positions[index]);

        lives.push(InstanceState {
            id,
            character: Arc::clone(character),
            director: StaticDirector::new(character.behaviors.clone(), seed ^ index as u64),
            // One per Instance, because each buddy wakes on its own clock and
            // carries its own conversation.
            //
            // ponytail: N Instances with a key make N times the model calls, on
            // N independent `Pace` clocks and against no shared budget. Fine for
            // the handful a desktop holds; a budget the Instances draw from is
            // the upgrade, and it wants somewhere to show the spend, which is
            // #18's panel.
            model: config.configured.then(|| {
                Arc::new(ModelDirector::new(
                    model::endpoint_from(settings).expect("configured means a Completer exists"),
                    character.behaviors.keys().cloned(),
                ))
            }),
            pending: model::InFlight::new(),
            in_flight: None,
            recent: Vec::new(),
            pace: Pace::with_growth(
                config.ambient_first,
                character.model_base,
                character.model_power,
            ),
            // Started somewhere inside the interval rather than at nothing, so
            // that N buddies do not all decide on the same tick. What each
            // decides already differs — every Instance has its own seed — but
            // deciding together still reads as coordinated, and it puts N model
            // calls in one instant instead of spreading them.
            since_wake: phase_of(config.wake_every, phases.draw()),
            since_state: Duration::ZERO,
            since_ambient: Duration::ZERO,
            previous_idle: Duration::MAX,
            last_state: None,
            addressed: false,
            happened: Happened::Ambient,
            pointer: Pointer::default(),
            spoken: None,
            drawn_last: None,
            verbs: Vec::new(),
            menu_hold: None,
        });
    }

    (roster, lives)
}

/// How far through the wake interval an Instance's clock starts, from one draw
/// of randomness.
///
/// Somewhere in the interval rather than a share of it apiece. An even spread
/// keeps buddies exactly out of phase, which is its own kind of mechanical: the
/// same three buddies wake in the same order at the same spacing for the whole
/// session, and two of them are always the same distance apart. A draw each
/// makes the spacing uneven and different every launch.
///
/// Never the whole interval, so nobody starts already due and wakes on the
/// first tick. The randomness is injected rather than drawn here so that the
/// arithmetic is testable — `docs/SPEC.md`, and the reason `Seeded` exists.
fn phase_of(interval: Duration, draw: u64) -> Duration {
    let millis = u64::try_from(interval.as_millis()).unwrap_or(u64::MAX);
    if millis == 0 {
        return Duration::ZERO;
    }
    Duration::from_millis(draw % millis)
}

/// How wide a Character's sprite usually is, in points.
///
/// The idle Animation's frame, blown up by the Character's scale. Animations may
/// declare different frame sizes, so this is what the Character is usually drawn
/// at rather than what it is always drawn at.
fn sprite_width(character: &Character) -> f64 {
    character
        .draw("idle", 0)
        .map_or(0.0, |drawn| f64::from(drawn.frame_size.0))
        * f64::from(character.scale)
}

/// Where each Instance comes into the world, given one starting point and each
/// sprite's width.
///
/// `starting_position` returns a single point, and several buddies dropped on it
/// would fall as one body and land in a stack. Each is placed a sprite's width
/// past the one before it — cumulatively, so the gap is the width of the sprite
/// actually standing there rather than the width of the newest: stepping by the
/// current Character's width puts a narrow one on top of the wide one it
/// follows.
///
/// Nothing clamps the run to the display. Far enough out and the Engine's walls
/// stop them, which is a better answer than arithmetic here pretending to know
/// how many will fit.
fn starting_positions(start: Point, widths: &[f64]) -> Vec<Point> {
    let mut x = start.x;
    widths
        .iter()
        .map(|width| {
            let at = Point { x, y: start.y };
            x += width;
            at
        })
        .collect()
}

/// The folder stem (`trump`) or the Character name (`Trump`) both name a package.
fn names_the_package(path: &Path, character_name: &str, wanted: &OsStr) -> bool {
    path.file_stem() == Some(wanted) || OsStr::new(character_name) == wanted
}

/// The Character an Instance asked for: the first package that loads out of
/// every place ai-buddy looks. Naming none takes the default.
///
/// Every rejection is reported before moving on, because a package that was
/// meant to load and did not is exactly what its author needs to hear about. A
/// location that was never a package is not worth a line.
///
/// Finding none stops startup, so the failure names every directory that was
/// searched: that list is the whole of what the reader has to go on.
///
/// Takes the name rather than reading the environment itself because an Instance
/// names its own Character. Several Instances mean several loads, and the
/// search, the reporting and the failure are the same for each.
fn load_named(
    app: &tauri::AppHandle,
    wanted: Option<std::ffi::OsString>,
) -> Result<Character, String> {
    // The shipped Characters are an app resource, which `tauri-build` copies
    // next to the binary for `cargo run` as well as into a bundle.
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(BUNDLED_CHARACTERS));

    let search_paths = package::search_paths(bundled);
    let installed = package::installed(&search_paths);
    let candidates = match &wanted {
        Some(name) => {
            // Folder stem first (`trump`). A switch persists Character.name
            // (`Trump`); that is not a stem, so fall through to every package
            // and match after load.
            let by_stem = package::named(installed.clone(), Some(name));
            if by_stem.is_empty() {
                installed
            } else {
                by_stem
            }
        }
        None => package::preferring(installed, package::DEFAULT_CHARACTER),
    };

    for candidate in &candidates {
        let files = match package::read(candidate) {
            Ok(files) => files,
            Err(package::ReadError::NotAPackage(_)) => continue,
            Err(why) => {
                eprintln!("character: {why}");
                continue;
            }
        };

        match ai_buddy_core::character::load(&files) {
            Ok(character) => {
                if let Some(wanted) = &wanted {
                    if !names_the_package(candidate, &character.name, wanted) {
                        continue;
                    }
                }
                eprintln!("character: {} from {}", character.name, candidate.display());
                return Ok(character);
            }
            // A rejection like any other: one broken package should not cost
            // the user every Character behind it in the search.
            Err(errors) => eprintln!(
                "character: {} is not a valid Character Package:\n  - {}",
                candidate.display(),
                errors.join("\n  - ")
            ),
        }
    }

    let looked_in = search_paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Which of the two failed matters: a Character that is not installed is a
    // typo in the name, and every Character failing to load is a broken build.
    Err(match wanted {
        Some(wanted) => format!(
            "no Character Package named {} loaded. ai-buddy looked in: {looked_in}",
            wanted.to_string_lossy()
        ),
        None => format!("no Character Package loaded. ai-buddy looked in: {looked_in}"),
    })
}

fn main() {
    // Same Completer, no overlay. scripts/probe-model.sh is the face of this.
    if std::env::args().any(|arg| arg == "--probe-model") {
        std::process::exit(model::run_probe());
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            character,
            overlay_primary,
            overlay_secondary
        ])
        .setup(|app| {
            // A companion with no Character has nothing to be, so no Character
            // means no overlay. Reported and exited rather than returned as a
            // setup error: Tauri turns that into a panic the event loop cannot
            // unwind, which buries the one line worth reading under a
            // backtrace. The same goes for a list of Instances that cannot be
            // read: starting with a different set of buddies from the one the
            // user asked for would be worse than saying so.
            let settings_file = settings::settings_path(&memory::data_dir());
            let mut settings = Settings::load(&settings_file);
            consent::set_wanted(
                consent::CapabilityId::Accessibility,
                settings.use_accessibility,
            );
            consent::set_wanted(
                consent::CapabilityId::ScreenRecording,
                settings.use_screen_recording,
            );
            let wanted = requested_instances(&settings).unwrap_or_else(|why| {
                eprintln!("instances: {why}");
                std::process::exit(1);
            });
            let loaded =
                load_instances(&app.handle().clone(), &wanted, &settings).unwrap_or_else(|why| {
                    eprintln!("character: {why}");
                    std::process::exit(1);
                });

            // Every installed package's art, so a switch or a spawn does not
            // have to wait for a reload the overlay never does.
            let (art, character_cache) = load_all_characters(&app.handle().clone());
            let mut characters = art;
            for (_, character) in &loaded {
                characters
                    .entry(character.name.clone())
                    .or_insert_with(|| CharacterArt {
                        art: art_urls(character),
                        smooth: character.smooth,
                    });
            }
            app.manage(ArtUrls { characters });
            let installed: Vec<String> = character_cache.keys().cloned().collect();

            // Keep ai-buddy out of the Dock and the application switcher. The
            // overlay is furniture, not an app you switch to.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Read before the overlays are built rather than after the loop
            // starts: reading which part of a display is usable means asking
            // AppKit, and only the main thread may do that.
            let (source, displays) = platform::window_source(app.handle().clone());
            let start = starting_position(&source.snapshot());

            // Which Dock the physics got: the true rectangle over the SPI,
            // the true rectangle over Accessibility, or the full-width strip
            // the work area reserves. Printed because the difference is
            // invisible until a sprite walks past the Dock's real end.
            if cfg!(target_os = "macos") {
                match displays.read().dock {
                    Some((dock, source)) => eprintln!(
                        "dock: true bounds via {source:?}, {}x{} at {},{}",
                        dock.width, dock.height, dock.x, dock.y
                    ),
                    None => eprintln!(
                        "dock: full-width floor; no source reported a bottom Dock — \
                         a side or hidden Dock has nothing to report, and granting \
                         ai-buddy Accessibility only helps where one exists"
                    ),
                }
            }

            // One overlay per display, so a Character straddling a seam is
            // drawn whole. The frame loop keeps the set in step with a desktop
            // that gains or loses a display.
            let covered = displays.read().frames;
            if covered.is_empty() {
                return Err("no displays reported".into());
            }
            place_overlays(app.handle(), &covered)?;

            // The sprite size is the first Instance's idle Animation, blown up.
            // Animations may declare different frame sizes, so this is what the
            // Character is usually drawn at rather than what it is always drawn
            // at; it is here because scripts/verify-overlay.sh crops a
            // screenshot to it, and that script runs one Instance.
            let (sprite_width, sprite_height) = loaded
                .first()
                .and_then(|(_, character)| {
                    let scale = character.scale as i32;
                    character.draw("idle", 0).map(|drawn| {
                        (
                            drawn.frame_size.0 as i32 * scale,
                            drawn.frame_size.1 as i32 * scale,
                        )
                    })
                })
                .unwrap_or((0, 0));

            eprintln!(
                "overlay: {} display(s); sprite {}x{}; {}",
                covered.len(),
                sprite_width,
                sprite_height,
                loaded
                    .iter()
                    .map(|(spec, character)| format!("{} as {}", character.name, spec.name))
                    .collect::<Vec<_>>()
                    .join(", "),
            );

            // Shared because the hotkey and the frame loop each see half of the
            // answer: the key is pressed on the main thread and the desktop is
            // read on the loop's.
            let mut hide_rules = HideRules::default();
            hide_rules.set_away(settings.hidden);
            hide_rules.set_hide_in_fullscreen(settings.hide_in_fullscreen);
            let rules = Arc::new(Mutex::new(hide_rules));

            if let Err(why) = app
                .handle()
                .plugin(tauri_plugin_updater::Builder::new().build())
            {
                eprintln!("updater: {why}");
            } else {
                check_for_update(app.handle().clone());
            }

            let secrets: Arc<dyn SecretStore> = Arc::new(KeyringStore::new());
            let director = match settings::director_settings(&settings, secrets.as_ref()) {
                Ok(director) => director,
                Err(why) => {
                    eprintln!("director: secret store: {why}");
                    model::resolve(&settings.director_base_url, &settings.director_model, None)
                }
            };
            let mut config = model::config_from(&director);
            config.enabled = settings.director_enabled && config.configured;
            config.ambient_allowed = settings.ambient_wakes;
            let inspect = Arc::new(Mutex::new(config.inspect()));
            app.manage(Arc::clone(&inspect));
            for line in model::startup_lines(&config) {
                eprintln!("{line}");
            }
            if config.enabled {
                model::spawn_preflight(&director);
            }

            let (mut roster, lives) = spawn_instances(&loaded, start, &config, &director);
            if settings.do_not_disturb {
                for (id, _) in roster.list() {
                    if let Some(instance) = roster.get_mut(&id) {
                        instance.set_do_not_disturb(true);
                    }
                }
            }
            if settings.character.is_empty() {
                if let Some((_, character)) = loaded.first() {
                    settings.character = character.name.clone();
                }
            }
            persist_settings(&settings, &settings_file);

            let settings = Arc::new(Mutex::new(settings));
            if install_hide_hotkey(
                app.handle(),
                Arc::clone(&rules),
                Arc::clone(&settings),
                settings_file.clone(),
            ) {
                let spec = settings
                    .lock()
                    .ok()
                    .map(|s| s.hide_hotkey.clone())
                    .unwrap_or_default();
                bind_hide_hotkey(app.handle(), &spec);
            }
            let instance_rows = Arc::new(Mutex::new(Vec::new()));
            let (ops_tx, ops_rx) = mpsc::channel();
            app.manage(SettingsState {
                settings: Arc::clone(&settings),
                path: settings_file.clone(),
                memory_path: memory::shared_path(),
                installed,
                instances: Arc::clone(&instance_rows),
                inspect: Arc::clone(&inspect),
                ops: ops_tx,
                rules: Arc::clone(&rules),
                secrets: Arc::clone(&secrets),
            });
            app.manage(Arc::clone(&rules));

            let tray = {
                let installed: Vec<String> = character_cache.keys().cloned().collect();
                let current = lives
                    .first()
                    .map(|live| live.character.name.clone())
                    .unwrap_or_default();
                let id = lives
                    .first()
                    .map(|live| live.id.clone())
                    .unwrap_or_default();
                let settings_now = settings.lock().ok().map(|s| s.clone()).unwrap_or_default();
                let rules_now = rules.lock().ok();
                let description = describe_menu(
                    &installed,
                    &current,
                    &roster,
                    &id,
                    &settings_now,
                    rules_now.as_deref().unwrap_or(&HideRules::default()),
                );
                match tray::install(app.handle(), &description) {
                    Ok(icon) => Some(icon),
                    Err(why) => {
                        eprintln!("tray: {why}");
                        None
                    }
                }
            };
            app.manage(TrayHandle(Mutex::new(tray)));

            let director_run = DirectorRun {
                config,
                settings: director,
                inspect,
            };

            // Selections do not come back from the popup: it returns once the
            // menu is on screen, and the click arrives here, later, on the app's
            // own channel. The hook forwards ids to the frame loop, which is the
            // only place that knows which Instance's menu is open and what its
            // rows meant.
            let (menu_sender, menu_receiver) = mpsc::channel();
            let hook_sender = menu_sender.clone();
            app.handle().on_menu_event(move |_app, event| {
                let id = event.id().0.clone();
                if id == menu::QUIT_ID {
                    quit_now();
                }
                let _ = hook_sender.send(MenuSignal::Chose(id));
            });

            run_frame_loop(
                app.handle().clone(),
                roster,
                lives,
                source,
                displays,
                rules,
                covered,
                director_run,
                MenuChannel {
                    sender: menu_sender,
                    receiver: menu_receiver,
                },
                FrameExtras {
                    settings,
                    settings_path: settings_file,
                    characters: character_cache,
                    instances: instance_rows,
                    ops: ops_rx,
                },
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ai-buddy failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_buddy_core::character::{PackageBytes, CHARACTER_MANIFEST_FILE, REQUIRED_ANIMATIONS};

    /// Settings persist Character.name (`Trump`). The env var and the folder
    /// are still the package stem (`trump`). Either has to start the same buddy.
    #[test]
    fn a_package_answers_to_its_folder_or_its_character_name() {
        let folder = Path::new("/characters/trump");
        assert!(
            names_the_package(folder, "Trump", OsStr::new("trump")),
            "AI_BUDDY_CHARACTER=trump still names the folder"
        );
        assert!(
            names_the_package(folder, "Trump", OsStr::new("Trump")),
            "settings.character after a switch is the Character name"
        );
        assert!(
            !names_the_package(Path::new("/characters/bmo"), "BMO", OsStr::new("Trump")),
            "some other package is not a match just because it loaded"
        );
    }

    /// A rebound hide hotkey must register the letter the user named, not B.
    #[test]
    fn a_rebound_spec_registers_its_letter() {
        let shortcut = shortcut_from_spec("Control-Shift-H").expect("parses");
        assert_eq!(shortcut.key, Code::KeyH);
        let shipped = shortcut_from_spec(settings::DEFAULT_HIDE_HOTKEY).expect("default");
        assert_eq!(shipped.key, Code::KeyB);
    }

    /// A 2x2 RGBA frame whose top-left pixel is transparent.
    const PATCHY: &[u8] = include_bytes!("../../crates/core/tests/fixtures/alpha-2x2.png");

    /// A 2x2 RGBA frame with every pixel drawn, so its URL is told apart from
    /// `PATCHY`'s.
    const SOLID: &[u8] = include_bytes!("../../crates/core/tests/fixtures/opaque-2x2.png");

    /// One Animation as these tests declare it: its name, then each frame as a
    /// file name and the bytes behind it.
    type Declared<'a> = (&'a str, &'a [(&'a str, &'a [u8])]);

    /// A Character whose Animations are `animations`, plus one frame each for
    /// every required Animation they do not name.
    fn character_declaring(animations: &[Declared<'_>]) -> Character {
        let mut manifest = String::from("name = \"Blip\"\n");
        let mut files = PackageBytes::new();

        let mut declare = |name: &str, frames: &[(&str, &[u8])]| {
            let names: Vec<String> = frames.iter().map(|(file, _)| format!("{file:?}")).collect();
            manifest.push_str(&format!(
                "[animations.{name}]\nframes = [{}]\n",
                names.join(", ")
            ));
            for (file, bytes) in frames {
                files.insert((*file).to_string(), bytes.to_vec());
            }
        };

        for required in REQUIRED_ANIMATIONS {
            if !animations.iter().any(|(name, _)| *name == required) {
                declare(required, &[(&format!("{required}.png"), PATCHY)]);
            }
        }
        for (name, frames) in animations {
            declare(name, frames);
        }

        files.insert(CHARACTER_MANIFEST_FILE.to_string(), manifest.into_bytes());
        ai_buddy_core::character::load(&files).expect("the package is valid")
    }

    /// The `data:` URL the art should carry for a frame of these bytes.
    fn url(bytes: &[u8]) -> String {
        format!("data:image/png;base64,{}", STANDARD.encode(bytes))
    }

    /// The invariant `art_urls` exists to hold: the webview indexes this list
    /// by the index the frame loop computed over `Animation::frames`, so a
    /// dropped or reordered URL would put a different frame on screen from the
    /// one the hit-test measured.
    #[test]
    fn an_animations_urls_stand_in_the_order_its_frames_do() {
        let character = character_declaring(&[(
            "walk",
            &[("a.png", PATCHY), ("b.png", SOLID), ("c.png", PATCHY)],
        )]);
        let art = art_urls(&character);

        assert_eq!(art["walk"], vec![url(PATCHY), url(SOLID), url(PATCHY)]);
        assert_eq!(art["walk"].len(), character.animations["walk"].frames.len());
    }

    /// The frame two Animations share is encoded once and named twice, at each
    /// Animation's own index: a shared URL that only appeared once would shift
    /// every later frame of the second Animation.
    #[test]
    fn a_frame_two_animations_share_stands_at_each_animations_own_index() {
        let character = character_declaring(&[
            ("idle", &[("shared.png", PATCHY), ("bob.png", SOLID)]),
            ("sit", &[("down.png", SOLID), ("shared.png", PATCHY)]),
        ]);
        let art = art_urls(&character);

        assert_eq!(art["idle"], vec![url(PATCHY), url(SOLID)]);
        assert_eq!(art["sit"], vec![url(SOLID), url(PATCHY)]);
    }

    /// Every way of starting ai-buddy that existed before Instances did asks for
    /// no Instances, which `load_instances` turns into the one buddy it has
    /// always run. One test rather than three because they read the same
    /// environment variable and separate tests would race each other for it.
    #[test]
    fn naming_no_instances_asks_for_none_and_a_list_is_read_in_full() {
        std::env::remove_var(INSTANCES_VAR);
        assert_eq!(
            requested_instances(&Settings::default()),
            Ok(Vec::new()),
            "the default single buddy is not a spec"
        );

        std::env::set_var(INSTANCES_VAR, "bmo:One,bmo:Two");
        let specs = requested_instances(&Settings::default()).expect("the list parses");
        assert_eq!(specs.len(), 2, "both Instances are asked for");
        assert!(specs.iter().all(|spec| spec.character == "bmo"));

        // A list that cannot be read stops startup rather than guessing.
        std::env::set_var(INSTANCES_VAR, "bmo:");
        assert!(requested_instances(&Settings::default()).is_err());

        std::env::remove_var(INSTANCES_VAR);

        let remembered = Settings {
            instances: vec![InstanceSpec {
                character: "nim".to_string(),
                name: "Nim".to_string(),
            }],
            ..Settings::default()
        };
        assert_eq!(
            requested_instances(&remembered).expect("settings list"),
            remembered.instances,
            "settings own the roster when the env is unset"
        );
    }

    /// The arithmetic that keeps buddies from landing in a stack, and the reason
    /// it accumulates: stepping by each Character's own width puts a narrow
    /// sprite on top of the wide one it follows.
    #[test]
    fn each_instance_starts_a_sprites_width_past_the_one_before_it() {
        let start = Point { x: 100.0, y: 50.0 };

        assert_eq!(
            starting_positions(start, &[32.0, 32.0, 32.0]),
            vec![
                Point { x: 100.0, y: 50.0 },
                Point { x: 132.0, y: 50.0 },
                Point { x: 164.0, y: 50.0 },
            ]
        );

        // A wide Character followed by a narrow one: the gap is the width of the
        // sprite standing there, not the width of the one arriving.
        assert_eq!(
            starting_positions(start, &[128.0, 16.0, 16.0]),
            vec![
                Point { x: 100.0, y: 50.0 },
                Point { x: 228.0, y: 50.0 },
                Point { x: 244.0, y: 50.0 },
            ],
            "the narrow sprite clears the wide one"
        );

        assert_eq!(
            starting_positions(start, &[]),
            Vec::new(),
            "no Instances, no positions"
        );
        assert_eq!(
            starting_positions(start, &[64.0]),
            vec![start],
            "one buddy still comes into the world where it always did"
        );
    }

    /// A wake clock starts somewhere inside the interval, never past it.
    #[test]
    fn a_wake_clock_starts_somewhere_inside_the_interval() {
        let interval = Duration::from_secs(60);

        assert_eq!(phase_of(interval, 0), Duration::ZERO);
        assert_eq!(phase_of(interval, 1_500), Duration::from_millis(1_500));

        // A draw is a whole u64, so most of them are past the interval and wrap.
        assert_eq!(
            phase_of(interval, 60_000),
            Duration::ZERO,
            "a draw of exactly the interval wraps to the start of it"
        );
        assert_eq!(phase_of(interval, 61_234), Duration::from_millis(1_234));

        // Never already due: a phase equal to the interval would wake every
        // buddy on the first tick, which is the thing being avoided.
        for draw in [0, 1, u64::MAX / 2, u64::MAX] {
            assert!(
                phase_of(interval, draw) < interval,
                "draw {draw} lands inside the interval"
            );
        }

        // An interval of nothing cannot happen, and must not divide by zero.
        assert_eq!(phase_of(Duration::ZERO, u64::MAX), Duration::ZERO);
    }

    /// The property the randomness is for: buddies from one launch start their
    /// clocks at different, unevenly spaced points.
    #[test]
    fn buddies_from_one_launch_start_their_clocks_apart() {
        let interval = Duration::from_secs(60);
        let mut draws = Seeded::new(0x5EED);

        let phases: Vec<Duration> = (0..4).map(|_| phase_of(interval, draws.draw())).collect();

        let mut distinct = phases.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            4,
            "no two buddies wake together: {phases:?}"
        );

        // Uneven, which is what a draw buys over a share apiece: an even spread
        // would make every gap identical.
        let gaps: Vec<Duration> = distinct.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            gaps.windows(2).any(|pair| pair[0] != pair[1]),
            "the spacing is not a fixed step: {gaps:?}"
        );
    }

    /// #178: a line said on one display and carried across the seam is said
    /// again to the new owner, once, while it could still be showing — and
    /// never to the owner that already heard it.
    #[test]
    fn a_line_crosses_the_seam_with_the_sprite_once_and_only_while_fresh() {
        let t0 = Instant::now();
        let mut spoken = None;

        assert_eq!(
            carry_line(&mut spoken, Some("Yare yare daze."), Some(0), t0).as_deref(),
            Some("Yare yare daze."),
            "the pulse itself goes to the owner of the tick"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(0), t0 + Duration::from_secs(1)),
            None,
            "the same owner is not told twice"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(1), t0 + Duration::from_secs(2)).as_deref(),
            Some("Yare yare daze."),
            "mid-reading, the new owner is told the line"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(1), t0 + Duration::from_secs(3)),
            None,
            "and then not again while it stays there"
        );
        assert_eq!(
            carry_line(
                &mut spoken,
                None,
                Some(0),
                t0 + CARRY_WINDOW + Duration::from_secs(1)
            ),
            None,
            "a line older than any reading window is not resurrected by a crossing"
        );

        let mut spoken = None;
        carry_line(&mut spoken, Some("first"), Some(0), t0);
        assert_eq!(
            carry_line(
                &mut spoken,
                Some("second"),
                Some(0),
                t0 + Duration::from_secs(1)
            )
            .as_deref(),
            Some("second"),
            "a new line replaces the remembered one"
        );
        assert_eq!(
            carry_line(&mut spoken, None, Some(1), t0 + Duration::from_secs(2)).as_deref(),
            Some("second"),
            "and it is the new line that crosses"
        );
    }
}
