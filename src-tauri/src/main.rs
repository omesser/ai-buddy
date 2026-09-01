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

mod env_util;
mod menu;
mod model;
mod package;
mod platform;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy_core::character::Character;
use ai_buddy_core::director::{
    self, Context, Director, Happened, ModelDirector, Pace, Seeded, StaticDirector, Wake,
};
use ai_buddy_core::engine::{Point, State, Verb};
use ai_buddy_core::input::{press_target, Pointer};
use ai_buddy_core::memory::{self, MemoryManifest};
use ai_buddy_core::overlay::{display_index_for, place_sprite, SpriteRect};
use ai_buddy_core::roster::{self, InstanceId, InstanceSpec, Roster};
use ai_buddy_core::sensing::{Activity, FreeTier, SystemClock};
use ai_buddy_core::snapshot::{starting_position, SnapshotAssembler};
use ai_buddy_core::visibility::{fullscreen_frontmost, Change, Desktop, HideRules};
use ai_buddy_core::window_source::{Rect, WindowSource};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Serialize;
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

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
    drawn_last: Option<Drawn>,
    /// This tick's verbs, decided before any Instance is ticked. Held on the
    /// Instance because `press_target` has to see every hit-test before any
    /// pointer is told whether the press was its own.
    verbs: Vec<Verb>,
    /// Channel receiver for async menu result. None when no menu is outstanding.
    /// While Some, the frame loop keeps feeding Verb::Menu to hold the instance.
    menu_pending: Option<std::sync::mpsc::Receiver<Option<menu::MenuAction>>>,
}

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

/// Last Character Prompt and current Director config. #18's settings panel
/// calls this.
#[tauri::command]
fn director_payload(
    inspect: tauri::State<'_, Arc<Mutex<model::DirectorInspect>>>,
) -> model::DirectorInspect {
    inspect
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
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

    // Click-through until the cursor is proven to be over the sprite. The wrong
    // default swallows a click; this one loses nothing.
    window.set_ignore_cursor_events(true)?;
    cover_display(&window, display)?;
    platform::configure_overlay(&window)?;
    window.show()?;

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
/// on the machine and B alone belongs to most of them. Fixed rather than
/// configurable: ai-buddy has no settings surface until #18, and a value that
/// never changes is not configuration.
///
/// A hotkey another application already holds is reported and let go. Losing it
/// costs the user one way to hide the Character, which is not worth losing the
/// Character over.
fn register_hide_hotkey(app: &tauri::AppHandle, rules: Arc<Mutex<HideRules>>) {
    let shortcut = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER),
        Code::KeyB,
    );

    // Spelled out rather than taken from `shortcut.into_string()`, which says
    // "control+alt+super+KeyB". These are the names on a Mac keyboard, and this
    // is the one line that tells a user which keys to press. Edit both.
    const HIDE_HOTKEY: &str = "Control-Option-Command-B";

    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_shortcut(shortcut)
        .expect("a Shortcut built here converts into itself")
        .with_handler(move |_app, _shortcut, event| {
            // Pressed only. The handler is called again on release, and a
            // toggle that ran twice would hand the Character back before the
            // user had let go of the key.
            if event.state() == ShortcutState::Pressed {
                if let Ok(mut rules) = rules.lock() {
                    rules.toggle();
                }
            }
        })
        .build();

    if let Err(why) = app.plugin(plugin) {
        eprintln!("hotkey: {HIDE_HOTKEY} is unavailable, so the Character cannot be hidden by hand: {why}");
    }
}

/// Apply a menu action to the roster and hide rules.
fn apply_menu_action(
    action: menu::MenuAction,
    roster: &mut Roster,
    instance_id: &InstanceId,
    rules: &Arc<Mutex<HideRules>>,
) {
    match action {
        menu::MenuAction::SwitchCharacter(name) => {
            eprintln!("menu: switching to {name}");
            // ponytail: character switching lands with
            // #18's settings. The menu builds and the
            // action is recognized; persistence and the
            // actual switch are deferred.
        }
        menu::MenuAction::ToggleDnd => {
            if let Some(instance) = roster.get_mut(instance_id) {
                let new_state = !instance.do_not_disturb();
                instance.set_do_not_disturb(new_state);
                eprintln!("menu: DND {}", if new_state { "on" } else { "off" });
            }
        }
        menu::MenuAction::Hide => {
            if let Ok(mut r) = rules.lock() {
                r.toggle();
                eprintln!("menu: hiding");
            }
        }
    }
}

/// The frame loop: assemble a snapshot, tick the Engine, apply the `Frame`.
///
/// Applying a `Frame` is two things at once, which is why they share a loop.
/// The webview is told where to draw and the hit-test is told where the sprite
/// is, both out of one `Frame`, so the outline the hit-test measures belongs to
/// the Animation frame the user sees.
///
/// Position is where the two part. The webview draws one sample behind and
/// interpolates towards this one, so the hit-test rectangle leads what is on
/// screen by up to one tick. src/interpolate.js carries the measurements that
/// make that lag the cheaper half of the trade against a stuttering sprite.
// One over the clippy cap. Director config belongs here. Folding it into
// the other seven would mix a timer with window geometry.
#[allow(clippy::too_many_arguments)]
fn run_frame_loop(
    app: tauri::AppHandle,
    mut roster: Roster,
    mut lives: Vec<InstanceState>,
    source: impl WindowSource + Send + 'static,
    displays: platform::DisplayCache,
    rules: Arc<Mutex<HideRules>>,
    covered: Vec<Rect>,
    director_run: DirectorRun,
) {
    thread::spawn(move || {
        let mut assembler = SnapshotAssembler::new(source);
        let DirectorRun { config, inspect } = director_run;

        // Read once for every Instance: there is one desktop and one user, and
        // asking AppKit how long they have been idle once per buddy would be
        // the same answer bought several times.
        let mut free_tier = FreeTier::default();
        let activity_source = platform::activity_source();
        let mut since_sense = Duration::ZERO;
        let mut last_activity: Option<Activity> = None;

        // One click-through flag per overlay, `None` until that overlay's first
        // decision so the first tick always applies.
        let mut ignoring: Vec<Option<bool>> = vec![None; covered.len()];

        // The displays the overlays cover, as setup left them. Shared with the
        // main thread, which is the only place that can change what they cover
        // and so the only place that knows when this is true again.
        let covered = Arc::new(Mutex::new(covered));

        // Click-through is invisible: nothing on screen says whether the overlay
        // is currently swallowing clicks or passing them on. This trace is the
        // only way to watch the decision without a human clicking. Off unless
        // asked for; see scripts/verify-overlay.sh.
        let tracing = env_util::env_flag_is_on("AI_BUDDY_TRACE_HITTEST");

        // Likewise for the Frame: where the sprite is and what it is doing is
        // the loop's only output, and a screenshot cannot say whether it got
        // there by falling.
        let tracing_frames = env_util::env_flag_is_on("AI_BUDDY_TRACE_FRAMES");
        let mut ticks: u32 = 0;
        let mut last_tick = Instant::now();

        loop {
            thread::sleep(ENGINE_TICK);

            let Ok(cursor) = app.cursor_position() else {
                continue;
            };

            // The windowing layer reports the global cursor against the primary
            // display's scale factor, whichever display it is actually over, so
            // that factor is what undoes it. It arrives from the cache rather
            // than from a monitor here: asking a monitor its scale means asking
            // `NSScreen`, and only the main thread may do that.
            let displays = displays.read();
            let cursor_scale = displays.cursor_scale;

            // One flag per overlay, and the desktop can gain or lose one.
            ignoring.resize(displays.frames.len(), None);

            // Wall time since the last tick that reached the Engine, not since
            // the last turn of this loop: a tick that could not read the
            // platform skips without advancing anything, and the time it spent
            // still passed for the sprite. `SnapshotAssembler` caps what it
            // hands the Engine, so a long gap — a skipped read, a suspended
            // process, a slept machine — is absorbed rather than slingshot.
            let elapsed_ms = u32::try_from(last_tick.elapsed().as_millis()).unwrap_or(u32::MAX);
            last_tick = Instant::now();

            // The Engine works in points across every display, which is the
            // space the cursor reading becomes once its own scale is undone.
            let cursor_points = ai_buddy_core::engine::Point {
                x: cursor.x / cursor_scale,
                y: cursor.y / cursor_scale,
            };

            // The hit-test asks its question in that shared space rather than
            // in an overlay's. Every overlay is handed the same sprite in its
            // own coordinates, so the answer is the same whichever overlay it
            // is asked of, and asking once is one answer instead of one per
            // window that could disagree.
            let cursor_at = (
                cursor_points.x.round() as i32,
                cursor_points.y.round() as i32,
            );

            // A dismissed Instance is gone from the Roster, and its Shell state
            // goes with it. Dropped before anything is hit-tested rather than
            // skipped inside the loop: a pointer left behind still counts as a
            // gesture, and one mid-drag when its Instance was dismissed would
            // hold every other buddy's presses for as long as the button stayed
            // down.
            lives.retain(|live| roster.get(&live.id).is_some());

            // Last tick's answer, which is the right one: the art being
            // hit-tested is the art that was last drawn. A Character nobody can
            // see is not there to be pressed, so a click where it would have
            // been reaches the window underneath and pokes nothing.
            let visible = rules.lock().is_ok_and(|rules| rules.presence().visible);

            let pressed: Vec<bool> = lives
                .iter()
                .map(|live| {
                    visible
                        && live.drawn_last.as_ref().is_some_and(|last| {
                            live.character
                                .draw(last.animation, last.animation_ms)
                                .is_some_and(|art| {
                                    art.mask.hit(
                                        &last.rect,
                                        cursor_at.0,
                                        cursor_at.1,
                                        last.mirrored,
                                    )
                                })
                        })
                })
                .collect();

            // One cursor, several sprites, and at most one gesture. Decided
            // across every Instance before any of them is told, because two
            // overlapping sprites handed the same hit-test would both be picked
            // up by one press.
            let gesturing = lives.iter().position(|live| live.pointer.gesturing());
            let target = press_target(&pressed, gesturing);
            let held = platform::primary_button_down();
            let secondary_held = platform::secondary_button_down();

            for (index, live) in lives.iter_mut().enumerate() {
                // Only the Instance the press belongs to is told the cursor is
                // over it. The rest are still updated: a pointer that stopped
                // being told the time would measure the next gesture's velocity
                // over the gap.
                live.verbs = live.pointer.update(
                    target == Some(index),
                    held,
                    secondary_held,
                    cursor_points,
                    elapsed_ms,
                );

                if tracing_frames && !live.verbs.is_empty() {
                    eprintln!("verbs: {} {:?}", live.id, live.verbs);
                }

                // Menu is async: first Verb::Menu starts the popup on main thread,
                // subsequent ticks poll the channel and keep feeding Verb::Menu
                // to hold the instance. When result arrives, apply action.
                if let Some(ref receiver) = live.menu_pending {
                    // Poll the channel for menu result.
                    if let Ok(action_opt) = receiver.try_recv() {
                        live.menu_pending = None;
                        if let Some(action) = action_opt {
                            apply_menu_action(
                                action,
                                &mut roster,
                                &live.id,
                                &Arc::clone(&rules),
                            );
                        }
                    } else {
                        // Menu still outstanding: inject Menu verb to keep hold.
                        if !live.verbs.iter().any(|v| matches!(v, Verb::Menu)) {
                            live.verbs.push(Verb::Menu);
                        }
                    }
                } else if live.verbs.iter().any(|verb| matches!(verb, Verb::Menu)) {
                    // First Menu verb: start async popup on main thread.
                    let bundled = app
                        .path()
                        .resource_dir()
                        .ok()
                        .map(|dir| dir.join(BUNDLED_CHARACTERS));
                    let search_paths = package::search_paths(bundled);
                    let installed = package::installed(&search_paths)
                        .iter()
                        .filter_map(|path| {
                            path.file_stem()
                                .and_then(|name| name.to_str())
                                .map(|name| name.to_string())
                        })
                        .collect::<Vec<_>>();

                    let do_not_disturb = roster
                        .get(&live.id)
                        .map(|inst| inst.do_not_disturb())
                        .unwrap_or(false);

                    let built = menu::build(&installed, &live.character.name, do_not_disturb);

                    // Create channel for async result.
                    let (tx, rx) = std::sync::mpsc::channel();
                    live.menu_pending = Some(rx);

                    // Post menu popup to main thread.
                    let app_clone = app.clone();
                    app.run_on_main_thread(move || {
                        let result = menu::show_and_wait(&built);
                        // Result may fail if receiver is dropped (instance dismissed),
                        // which is fine: menu closes, result discarded.
                        let _ = tx.send(result);
                    })
                    .ok();
                }

                // Grab is on every held tick. Only the first tick of a hold is a
                // pick-up; the rest would otherwise wake the session while
                // dragging.
                let grab_started = live.verbs.iter().any(|verb| matches!(verb, Verb::Grab))
                    && live.last_state != Some(State::Dragged);
                if live.verbs.iter().any(|verb| {
                    matches!(
                        verb,
                        Verb::Poke | Verb::Summon | Verb::Menu | Verb::Throw { .. }
                    )
                }) || grab_started
                {
                    live.addressed = true;
                    live.happened = if live
                        .verbs
                        .iter()
                        .any(|verb| matches!(verb, Verb::Throw { .. }))
                    {
                        Happened::Throw
                    } else if grab_started {
                        Happened::Grab
                    } else if live
                        .verbs
                        .iter()
                        .any(|verb| matches!(verb, Verb::Menu | Verb::Poke))
                    {
                        Happened::Poke
                    } else {
                        Happened::Summon
                    };
                }
            }

            // The Director's clock is the same elapsed time the Engine is
            // given, so a loop that stalled wakes it once on the way back
            // rather than in a burst. Firing takes one interval off the clock
            // rather than zeroing it, for the reason `SnapshotAssembler` gives:
            // zeroing throws the overshoot away and stretches every interval by
            // most of a tick.
            let elapsed = Duration::from_millis(u64::from(elapsed_ms));
            since_sense += elapsed;

            // One reading of the user for every Instance, taken before any of
            // them is ticked so that they all wake against the same desktop. A
            // read per buddy would be the same two calls into AppKit bought N
            // times, and two buddies deciding on idle times a tick apart.
            let sensed = if since_sense >= SENSE_INTERVAL {
                since_sense = since_sense.saturating_sub(SENSE_INTERVAL);
                let activity = free_tier.read(&activity_source, &SystemClock);
                last_activity = Some(activity.clone());
                Some(activity)
            } else {
                None
            };
            let displays_asleep = last_activity
                .as_ref()
                .is_some_and(|activity| activity.displays_asleep);

            // Assembled once and handed to every Instance. It carries the
            // desktop, which they share, and the window list is re-read on the
            // assembler's own schedule — asking it once per Instance would poll
            // the window server N times for one answer.
            let mut world = assembler.assemble(elapsed_ms, cursor_points, Vec::new());

            // Whole display frames, not the usable ones physics runs in: the
            // reserved strips are the difference between a fullscreen window
            // and a zoomed one, which is the whole of what is being measured.
            // Rectangles only: whether a window has taken a whole screen is a
            // question about geometry, and `visibility` has no use for which
            // window it is.
            let rects: Vec<_> = world.windows.iter().map(|window| window.rect).collect();
            let desktop = Desktop {
                fullscreen_frontmost: fullscreen_frontmost(&rects, &displays.frames),
            };

            // Where each Instance ends up, in the space every display shares.
            let mut placed: Vec<Placed> = Vec::with_capacity(lives.len());

            // The window list is re-read at the frame rate while any Instance is
            // riding a moving window. One riding buddy is reason enough: the
            // others cost nothing extra, the read being shared.
            let mut riding = false;

            // Whether the cursor is over any Instance's art. Click-through is a
            // property of the overlay, which every Instance shares, so one
            // sprite under the cursor is enough to make the overlay take the
            // click — and the press is then routed to that one Instance.
            let mut over_sprite = false;

            for live in lives.iter_mut() {
                let Some(instance) = roster.get_mut(&live.id) else {
                    continue; // dismissed; the retain above has already dropped it
                };

                live.since_wake += elapsed;
                live.since_state += elapsed;
                if !displays_asleep {
                    live.since_ambient += elapsed;
                }

                let mut proposal = None;
                let arrived = live.pending.try_take();
                let applied = arrived.is_some();
                if let Some(wake) = arrived {
                    let context = live
                        .in_flight
                        .take()
                        .expect("a started call still has its context");
                    if model::tracing() {
                        match &wake {
                            Wake::Proposed(parsed) if !parsed.behavior.is_empty() => eprintln!(
                                "director: {} parsed {}{}",
                                live.id,
                                parsed.behavior,
                                parsed
                                    .dialogue
                                    .as_deref()
                                    .map(|line| format!(" | {line}"))
                                    .unwrap_or_default(),
                            ),
                            Wake::Proposed(_) => {}
                            Wake::Failed => {
                                eprintln!("director: {} failed; Static fallback", live.id)
                            }
                        }
                    }
                    proposal = director::fallback(wake, &mut live.director, &context);
                    if model::tracing() {
                        match &proposal {
                            Some(playing) if playing.behavior.is_empty() => {
                                eprintln!(
                                    "director: {} saying {}",
                                    live.id,
                                    playing.dialogue.as_deref().unwrap_or("")
                                );
                            }
                            Some(playing) => eprintln!(
                                "director: {} playing {}{}",
                                live.id,
                                playing.behavior,
                                playing
                                    .dialogue
                                    .as_deref()
                                    .map(|line| format!(" | {line}"))
                                    .unwrap_or_default(),
                            ),
                            None => eprintln!("director: {} nothing to play", live.id),
                        }
                    }
                }

                if let Some(activity) = &sensed {
                    let due = director::due(
                        live.since_wake,
                        config.wake_every,
                        activity,
                        live.previous_idle,
                        live.since_state,
                        instance.do_not_disturb(),
                    );
                    live.previous_idle = activity.idle;

                    if due {
                        live.since_wake = live.since_wake.saturating_sub(config.wake_every);
                        live.since_state = Duration::ZERO;
                        // Static keeps the free life going. A session call in
                        // flight is the one exception: do not stack a weight pick
                        // on a proposal that is about to land.
                        if live.pending.ready() && !applied {
                            proposal = live.director.propose(&Context {
                                activity: activity.clone(),
                                recent: live.recent.clone(),
                                personality: live.character.personality.clone(),
                                state: live.last_state.unwrap_or(State::Grounded),
                                happened: live.happened,
                                standing: String::new(),
                            });
                        }
                    }
                }

                // The verbs are this Instance's alone, decided above. Taken
                // rather than cloned: the snapshot is reused across Instances,
                // and a verb left behind would be replayed next tick.
                world.verbs = std::mem::take(&mut live.verbs);
                world.proposal = proposal;

                let frame = instance.tick(&world);
                riding |= frame.riding;

                let became_perched = live.last_state.is_some()
                    && frame.state == State::Perched
                    && live.last_state != Some(State::Perched);
                if became_perched {
                    live.addressed = true;
                    live.happened = Happened::Perch;
                }

                if live.last_state != Some(frame.state) {
                    live.last_state = Some(frame.state);
                    live.since_state = Duration::ZERO;
                }

                // After the tick so a Throw is already Falling, not still Dragged.
                let reactive_wake =
                    if let (Some(model), Some(activity)) = (&live.model, last_activity.as_ref()) {
                        if director::session_due(
                            live.addressed,
                            live.since_ambient,
                            &live.pace,
                            activity.displays_asleep,
                            instance.do_not_disturb(),
                        ) && live.pending.ready()
                            && !applied
                        {
                            let context = Context {
                                activity: activity.clone(),
                                recent: live.recent.clone(),
                                personality: live.character.personality.clone(),
                                state: frame.state,
                                happened: live.happened,
                                standing: assembler.standing_on(frame.position),
                            };
                            let was_addressed = live.addressed;
                            if live.addressed {
                                live.pace.after_reactive();
                            } else {
                                live.pace.after_ambient();
                            }
                            live.addressed = false;
                            live.happened = Happened::Ambient;
                            live.since_ambient = Duration::ZERO;
                            let payload = model.prompt(&context);
                            // One panel for however many Instances are running, so
                            // the newest call is what it shows. #18 owns the panel
                            // and can give it a buddy to choose; until then, the
                            // last payload sent is the honest answer to "what was
                            // sent", whichever Instance sent it.
                            if let Ok(mut inspect) = inspect.lock() {
                                inspect.last_payload = Some(payload);
                                inspect.wake_secs = live.pace.wait().as_secs();
                            }
                            live.pending.start(Arc::clone(model), context.clone());
                            live.in_flight = Some(context);
                            was_addressed
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                let thinking = !live.pending.ready()
                    && (reactive_wake
                        || live
                            .in_flight
                            .as_ref()
                            .is_some_and(|ctx| ctx.happened != Happened::Ambient))
                    && !instance.do_not_disturb();

                // What the user has seen is what the Engine played, not what the
                // Director asked for: a proposal the State refuses never reaches
                // the screen, and suppressing it would silence a Behavior nobody
                // watched.
                if let Some(played) = &frame.behavior {
                    director::remember(&mut live.recent, played.clone());
                    if tracing_frames {
                        eprintln!("director: {} {played}", live.id);
                    }
                }

                // The Engine names an Animation and how long it has been
                // playing; the Character Manifest says what that means in
                // frames. Resolving it here rather than in the webview keeps the
                // frame the hit-test measures and the frame the user sees the
                // same one.
                // A Character with no drawable Animation at all, which a
                // validated Character Package cannot be. Left out of `placed`,
                // and the webview reads absence as dismissal: it takes the
                // sprite away, bubble and interpolation with it. That is the
                // right answer for art that cannot be drawn, and the reason
                // nothing else in this loop may skip an Instance silently.
                let Some(drawn) = live.character.draw(frame.animation, frame.animation_ms) else {
                    continue;
                };
                let scale = live.character.scale as i32;
                let (width, height) = (
                    drawn.frame_size.0 as i32 * scale,
                    drawn.frame_size.1 as i32 * scale,
                );

                // Placed once, in the space every display shares. Each overlay
                // is handed it in its own coordinates below.
                let sprite =
                    place_sprite((frame.position.x, frame.position.y), (width, height), scale);

                if tracing_frames {
                    // Unix milliseconds, so that a prop window opened by the
                    // verification script and this loop can be read against one
                    // clock. Only read when tracing: the loop needs elapsed
                    // time, never the time of day.
                    let at_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |since| since.as_millis());

                    // The Instance last, so that everything before it stands
                    // where it did when one buddy was all there was.
                    // scripts/verify-overlay.sh matches on this prefix, runs a
                    // single Instance, and has no use for which one.
                    eprintln!(
                        "frame: {} {:?} pos({:.0},{:.0}) sprite({},{}) {}#{} {}",
                        at_ms,
                        frame.state,
                        frame.position.x,
                        frame.position.y,
                        sprite.x,
                        sprite.y,
                        frame.animation,
                        drawn.index,
                        live.id,
                    );
                }

                // The tick's second hit-test, against the sprite about to be
                // drawn rather than the one last drawn: whether the next click
                // should reach us is a question about where the art is going to
                // be. A cursor that has just arrived over it must not spend a
                // frame passing clicks to the application underneath.
                let mirrored = frame.facing < 0.0;
                over_sprite |= drawn.mask.hit(&sprite, cursor_at.0, cursor_at.1, mirrored);
                live.drawn_last = Some(Drawn {
                    rect: sprite,
                    animation: frame.animation,
                    animation_ms: frame.animation_ms,
                    mirrored,
                });

                placed.push(Placed {
                    id: live.id.clone(),
                    character: live.character.name.clone(),
                    sprite,
                    width,
                    height,
                    animation: drawn.animation.to_string(),
                    frame_index: drawn.index,
                    facing: frame.facing as i8,
                    dialogue: frame.dialogue.clone(),
                    thinking,
                });
            }

            assembler.poll_fast(riding);

            // The log is what is silent on almost every tick, not the
            // renderer: only a change is worth a line, and a fullscreen
            // application held for an hour is one of them rather than one an
            // Engine tick.
            let presence = rules
                .lock()
                .map(|mut rules| {
                    if let Some(change) = rules.update(desktop) {
                        // Unconditional, unlike the traces above, because it is
                        // rare — a handful of lines in a session — and because
                        // whether a rule fired is the first thing anyone
                        // checking hiding by hand needs to know.
                        eprintln!(
                            "presence: {} over {}ms",
                            if change.visible { "shown" } else { "hidden" },
                            change.fade_ms,
                        );
                    }
                    rules.presence()
                })
                .unwrap_or(Change {
                    visible,
                    fade_ms: 0,
                });

            // A display can be plugged in, unplugged or rearranged while the
            // app runs, and every display needs its overlay. Posted rather than
            // done here: only the main thread may build a window.
            //
            // Recorded by the closure, on success, rather than here once the
            // post is accepted. An accepted post only means the work is queued,
            // and every failure inside `place_overlays` is a log line rather
            // than a refusal; recording it here would latch a desktop the
            // overlays never reached and leave a hot-plugged display with no
            // overlay until the display list changed again. The price is that a
            // reconcile that keeps failing is posted again every tick, which is
            // what retrying it means.
            //
            // An empty read is ignored rather than obeyed. It is what a failed
            // read of the desktop looks like as well as a machine with no
            // screen, and tearing every overlay down costs two webviews and
            // their art to rebuild — for a desktop that has nothing to draw on
            // either way.
            if !displays.frames.is_empty() && *covered.lock().unwrap() != displays.frames {
                let handle = app.clone();
                let frames = displays.frames.clone();
                let placed = Arc::clone(&covered);
                let _ = app.run_on_main_thread(move || match place_overlays(&handle, &frames) {
                    Ok(()) => *placed.lock().unwrap() = frames,
                    Err(why) => eprintln!("overlay: {why}"),
                });
            }

            // Click-through returns wherever no sprite is drawn, and everywhere
            // while the Character is hidden — a Character nobody can see must
            // not swallow a click. The exception is a held Character: a drag
            // that outruns the art would otherwise put the cursor over
            // transparent pixels, hand the button to whatever is underneath,
            // and drop the sprite in the user's hand.
            let holding = lives.iter().any(|live| live.pointer.grabbing());
            let ignore = !(presence.visible && (over_sprite || holding));
            let on_overlay =
                display_index_for((cursor_points.x, cursor_points.y), &displays.frames);
            let mut flipped = false;

            for (index, display) in displays.frames.iter().enumerate() {
                let label = overlay_label(index);
                let Some(window) = app.get_webview_window(&label) else {
                    continue; // a display whose overlay has not been built yet
                };
                // Every overlay is told about every Instance, including the ones
                // no sprite is anywhere near: each draws the part that falls
                // inside it, which is what leaves a Character on a seam whole
                // instead of clipped to one display.
                //
                // Addressed rather than emitted to all, because each overlay
                // is told a different set of rectangles. src/main.js has to name
                // its own label to match: an untargeted listener hears every
                // emit, addressed elsewhere or not, and would draw whichever
                // display's rectangles arrived last.
                let sprites = placed
                    .iter()
                    .map(|instance| {
                        let local = instance.sprite.in_overlay(*display);
                        SpritePlacement {
                            id: &instance.id,
                            character: &instance.character,
                            x: local.x,
                            y: local.y,
                            width: instance.width,
                            height: instance.height,
                            animation: &instance.animation,
                            frame_index: instance.frame_index,
                            facing: instance.facing,
                            dialogue: instance.dialogue.clone(),
                            thinking: instance.thinking,
                        }
                    })
                    .collect();

                let _ = window.emit_to(
                    label,
                    FRAME_EVENT,
                    Placement {
                        sprites,
                        visible: presence.visible,
                        fade_ms: presence.fade_ms,
                    },
                );

                // Click-through is per-window, and a click only ever lands on
                // the overlay the cursor is on. Every other overlay passes
                // clicks through whatever the sprite is doing, so a click on
                // one display is never swallowed by a sprite on another.
                let ignore = ignore || on_overlay != Some(index);

                // Only record the new state once the platform accepted it.
                // Recording it regardless would latch a failed toggle forever,
                // leaving click-through stuck in whichever mode it happened to
                // be in.
                if ignoring[index] != Some(ignore) {
                    flipped = true;
                    if window.set_ignore_cursor_events(ignore).is_ok() {
                        ignoring[index] = Some(ignore);
                    }
                }
            }

            ticks = ticks.wrapping_add(1);
            if tracing && (flipped || ticks % 120 == 0) {
                eprintln!(
                    "hit-test: cursor({:.0},{:.0}) scale {:.1} -> point({},{}) \
                     on overlay {} {} click-through {}{}",
                    cursor.x,
                    cursor.y,
                    cursor_scale,
                    cursor_at.0,
                    cursor_at.1,
                    on_overlay.map_or(-1, |index| index as i32),
                    if over_sprite { "HIT " } else { "miss" },
                    if ignore { "on" } else { "OFF" },
                    if flipped { "  <- flipped" } else { "" },
                );
            }
        }
    });
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
/// Empty is the answer when it asks for none, and it is not a failure: it is
/// every way of starting ai-buddy that existed before Instances did.
/// `load_instances` turns it into the one buddy the app has always run.
fn requested_instances() -> Result<Vec<InstanceSpec>, String> {
    roster::parse_specs(&std::env::var(INSTANCES_VAR).unwrap_or_default())
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
fn load_instances(
    app: &tauri::AppHandle,
    wanted: &[InstanceSpec],
) -> Result<Vec<(InstanceSpec, Arc<Character>)>, String> {
    if wanted.is_empty() {
        let character = Arc::new(load_named(app, std::env::var_os(package::CHARACTER_VAR))?);
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
            model: config.enabled.then(|| {
                Arc::new(ModelDirector::new(
                    model::endpoint().expect("enabled means a key was set"),
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
            drawn_last: None,
            verbs: Vec::new(),
            menu_pending: None,
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
        Some(_) => package::named(installed, wanted.as_deref()),
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
        .invoke_handler(tauri::generate_handler![character, director_payload])
        .setup(|app| {
            // A companion with no Character has nothing to be, so no Character
            // means no overlay. Reported and exited rather than returned as a
            // setup error: Tauri turns that into a panic the event loop cannot
            // unwind, which buries the one line worth reading under a
            // backtrace. The same goes for a list of Instances that cannot be
            // read: starting with a different set of buddies from the one the
            // user asked for would be worse than saying so.
            let wanted = requested_instances().unwrap_or_else(|why| {
                eprintln!("instances: {why}");
                std::process::exit(1);
            });
            let loaded = load_instances(&app.handle().clone(), &wanted).unwrap_or_else(|why| {
                eprintln!("character: {why}");
                std::process::exit(1);
            });

            // One art entry per Character, however many Instances run it.
            let mut characters = BTreeMap::new();
            for (_, character) in &loaded {
                characters
                    .entry(character.name.clone())
                    .or_insert_with(|| CharacterArt {
                        art: art_urls(character),
                        smooth: character.smooth,
                    });
            }
            app.manage(ArtUrls { characters });

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
            // invisible until a sprite walks past the Dock's real end, and
            // the app never prompts to change it (DESIGN.md decision 9).
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
            let rules = Arc::new(Mutex::new(HideRules::default()));
            register_hide_hotkey(app.handle(), Arc::clone(&rules));

            let config = model::config();
            let inspect = Arc::new(Mutex::new(config.inspect()));
            app.manage(Arc::clone(&inspect));
            if config.enabled {
                eprintln!(
                    "director: model, ambient first {}s",
                    config.ambient_first.as_secs()
                );
            } else if config.configured {
                eprintln!("director: off; using StaticDirector");
            }

            let (roster, lives) = spawn_instances(&loaded, start, &config);
            let director_run = DirectorRun { config, inspect };

            run_frame_loop(
                app.handle().clone(),
                roster,
                lives,
                source,
                displays,
                rules,
                covered,
                director_run,
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
            requested_instances(),
            Ok(Vec::new()),
            "the default single buddy is not a spec"
        );

        std::env::set_var(INSTANCES_VAR, "bmo:One,bmo:Two");
        let specs = requested_instances().expect("the list parses");
        assert_eq!(specs.len(), 2, "both Instances are asked for");
        assert!(specs.iter().all(|spec| spec.character == "bmo"));

        // A list that cannot be read stops startup rather than guessing.
        std::env::set_var(INSTANCES_VAR, "bmo:");
        assert!(requested_instances().is_err());

        std::env::remove_var(INSTANCES_VAR);
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
}
