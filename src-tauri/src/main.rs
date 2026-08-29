//! ai-buddy's overlay shell.
//!
//! One transparent, always-on-top window renders the Character. Click-through on
//! macOS is per-window rather than per-pixel, so a screen-sized transparent
//! window would swallow every click. The shell therefore tracks the cursor and
//! toggles ignore-mouse-events by hit-testing the sprite's alpha, which is what
//! makes the overlay feel like a sprite on the desktop instead of a sheet of
//! glass over it.
//!
//! It also owns the frame loop, which is the only thing that can: the Engine is
//! pure and cannot read a clock, and `WindowSource` reports geometry and nothing
//! else. The loop reads the wall clock and the cursor, asks
//! `SnapshotAssembler` for a `WorldSnapshot`, ticks the Engine, and hands the
//! resulting `Frame` to the webview and to the hit-test.

mod cast;
mod package;
mod platform;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy_core::engine::{Engine, Point};
use ai_buddy_core::input::Pointer;
use ai_buddy_core::overlay::{cursor_in_window, display_for, place_sprite, SpriteRect};
use ai_buddy_core::snapshot::{starting_position, SnapshotAssembler};
use ai_buddy_core::visibility::{fullscreen_frontmost, Change, Desktop, HideRules};
use ai_buddy_core::window_source::{Rect, WindowSource};
use serde::Serialize;
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

use cast::Cast;

/// Nearest-neighbour blow-up, in logical points. ADR-0006 permits integers only.
const SPRITE_SCALE: i32 = 4;

/// Where the shipped Character Packages sit inside the app's resources. Kept in
/// step with `bundle.resources` in `tauri.conf.json`.
const BUNDLED_CHARACTERS: &str = "characters";

/// Alpha at or above this counts as drawn. See `AlphaMask::from_png`.
const ALPHA_THRESHOLD: u8 = 128;

/// One turn of the frame loop: roughly 60Hz, six times the rate the desktop's
/// geometry is read at.
///
/// It is a poll rather than an event stream for two reasons. A click-through
/// window receives no mouse events at all, so the webview cannot tell us when
/// the cursor returns — something outside the window has to ask where it is.
/// And the Engine advances on elapsed time, so something has to advance it.
const ENGINE_TICK: Duration = Duration::from_millis(16);

const OVERLAY_LABEL: &str = "overlay";

/// The event carrying each `Frame` to the webview.
const FRAME_EVENT: &str = "frame";

/// Where the sprite was last drawn, and what it was drawn as.
///
/// Kept for one tick so the hit-test can ask about the sprite the user is
/// looking at rather than the one this tick is about to produce.
struct Drawn {
    rect: SpriteRect,
    animation: &'static str,
    animation_ms: u32,
}

/// One tick's instruction to the renderer: where to draw the sprite in logical
/// points from the overlay's top-left, and which Animation frame to draw.
///
/// Pushed every tick rather than fetched, so the webview holds no authoritative
/// state — it draws what it was last told and remembers nothing.
#[derive(Clone, Copy, Serialize)]
struct Placement {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    animation: &'static str,
    frame_index: usize,
    /// Whether the hide rules have the Character on screen, and how long the
    /// change that decided it was given.
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

/// The Character's art, fetched once by the webview when it loads.
///
/// A command rather than an event: an event emitted during setup would race the
/// webview's own listener, and the art does not change while the app runs.
#[tauri::command]
fn character(cast: tauri::State<'_, Arc<Cast>>) -> BTreeMap<String, Vec<String>> {
    cast.art().clone()
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
    // Named here rather than beside the other constants so the keys and the
    // words for them cannot drift apart.
    const HIDE_HOTKEY: &str = "Control-Option-Command-B";

    let shortcut = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER),
        Code::KeyB,
    );

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
fn run_frame_loop(
    app: tauri::AppHandle,
    cast: Arc<Cast>,
    source: impl WindowSource + Send + 'static,
    displays: platform::DisplayCache,
    rules: Arc<Mutex<HideRules>>,
    start: Point,
    home: Rect,
) {
    thread::spawn(move || {
        let mut engine = Engine::new(start).with_behaviors(cast.behaviors().clone());
        let mut assembler = SnapshotAssembler::new(source);

        // The display the overlay is currently over, as setup left it.
        let mut home = home;

        // `None` until the first decision, so the first tick always applies.
        let mut ignoring: Option<bool> = None;

        let mut pointer = Pointer::default();

        // Click-through is invisible: nothing on screen says whether the overlay
        // is currently swallowing clicks or passing them on. This trace is the
        // only way to watch the decision without a human clicking. Off unless
        // asked for; see scripts/verify-overlay.sh.
        let tracing = std::env::var_os("AI_BUDDY_TRACE_HITTEST").is_some();

        // Likewise for the Frame: where the sprite is and what it is doing is
        // the loop's only output, and a screenshot cannot say whether it got
        // there by falling.
        let tracing_frames = std::env::var_os("AI_BUDDY_TRACE_FRAMES").is_some();
        let mut ticks: u32 = 0;
        let mut last_tick = Instant::now();

        // The art is looked up again rather than kept, which costs one map
        // lookup and saves copying a mask sixty times a second.
        let mut drawn_last: Option<Drawn> = None;

        loop {
            thread::sleep(ENGINE_TICK);

            let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
                return; // window is gone; so is the reason to tick
            };

            let (Ok(cursor), Ok(origin), Ok(scale)) = (
                app.cursor_position(),
                window.outer_position(),
                window.scale_factor(),
            ) else {
                continue;
            };

            // The windowing layer reports the global cursor against the primary
            // display's scale factor but the window's origin against the
            // window's own, so each needs undoing with its own factor. The
            // primary's arrives from the cache rather than from a monitor here:
            // asking a monitor its scale means asking `NSScreen`, and only the
            // main thread may do that.
            let displays = displays.read();
            let cursor_scale = displays.cursor_scale;

            // Wall time since the last tick that reached the Engine, not since
            // the last turn of this loop: a tick that could not read the
            // platform skips without advancing anything, and the time it spent
            // still passed for the sprite. `SnapshotAssembler` caps what it
            // hands the Engine, so a long gap — a skipped read, a suspended
            // process, a slept machine — is absorbed rather than slingshot.
            let elapsed_ms = u32::try_from(last_tick.elapsed().as_millis()).unwrap_or(u32::MAX);
            last_tick = Instant::now();

            let (local_x, local_y) = cursor_in_window(
                (cursor.x, cursor.y),
                cursor_scale,
                (origin.x as f64, origin.y as f64),
                scale,
            );

            // Last tick's answer, which is the right one: the art being
            // hit-tested is the art that was last drawn. A Character nobody can
            // see is not there to be pressed, so a click where it would have
            // been reaches the window underneath and pokes nothing.
            let visible = rules.lock().is_ok_and(|rules| rules.presence().visible);

            let pressed_sprite = visible
                && drawn_last.as_ref().is_some_and(|last| {
                    cast.draw(last.animation, last.animation_ms)
                        .is_some_and(|art| art.mask.hit(&last.rect, local_x, local_y))
                });

            // The Engine works in points across every display, which is the
            // space the cursor reading becomes once its own scale is undone.
            let cursor_points = ai_buddy_core::engine::Point {
                x: cursor.x / cursor_scale,
                y: cursor.y / cursor_scale,
            };
            let verbs = pointer.update(
                pressed_sprite,
                platform::primary_button_down(),
                cursor_points,
                elapsed_ms,
            );

            if tracing_frames && !verbs.is_empty() {
                eprintln!("verbs: {verbs:?}");
            }
            let snapshot = assembler.assemble(elapsed_ms, cursor_points, verbs);

            // Whole display frames, not the usable ones physics runs in: the
            // reserved strips are the difference between a fullscreen window
            // and a zoomed one, which is the whole of what is being measured.
            let desktop = Desktop {
                fullscreen_frontmost: fullscreen_frontmost(&snapshot.windows, &displays.frames),
                do_not_disturb: platform::do_not_disturb(),
            };

            let frame = engine.tick(&snapshot);

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

            // The overlay covers one display and follows the Character to the
            // next. macOS gives each display its own Space and draws a window
            // that spans two of them on only one, so a window sized to the
            // display union is invisible everywhere but the display it happens
            // to belong to — which is how a Character dragged across a boundary
            // was lost while the Engine still had it in hand.
            //
            // The feet decide rather than the cursor, because a Throw crosses
            // with nobody holding it.
            //
            // ponytail: the sprite is clipped while it straddles the seam,
            // because one window cannot be on two displays. One overlay per
            // display would draw both halves; that is the upgrade if the pop
            // reads badly.
            if let Some(display) =
                display_for((frame.position.x, frame.position.y), &displays.frames)
            {
                // Recorded only once the move is accepted, so a refused one is
                // retried next tick rather than latched as done.
                if display != home && cover_display(&window, display).is_ok() {
                    home = display;
                }
            }

            // The Engine names an Animation and how long it has been playing;
            // the Character Manifest says what that means in frames. Resolving
            // it here rather than in the webview keeps the frame the hit-test
            // measures and the frame the user sees the same one.
            let Some(drawn) = cast.draw(frame.animation, frame.animation_ms) else {
                continue; // a Character with no drawable Animation at all
            };
            let (width, height) = (
                drawn.art_size.0 * SPRITE_SCALE,
                drawn.art_size.1 * SPRITE_SCALE,
            );

            let sprite = place_sprite(
                (frame.position.x, frame.position.y),
                (origin.x as f64, origin.y as f64),
                scale,
                (width, height),
                SPRITE_SCALE,
            );

            let _ = window.emit(
                FRAME_EVENT,
                Placement {
                    x: sprite.x,
                    y: sprite.y,
                    width,
                    height,
                    animation: frame.animation,
                    frame_index: drawn.index,
                    visible: presence.visible,
                    fade_ms: presence.fade_ms,
                },
            );

            if tracing_frames {
                // Unix milliseconds, so that a prop window opened by the
                // verification script and this loop can be read against one
                // clock. Only read when tracing: the loop needs elapsed time,
                // never the time of day.
                let at_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| since.as_millis());

                eprintln!(
                    "frame: {} {:?} pos({:.0},{:.0}) sprite({},{}) {}#{}",
                    at_ms,
                    frame.state,
                    frame.position.x,
                    frame.position.y,
                    sprite.x,
                    sprite.y,
                    frame.animation,
                    drawn.index,
                );
            }

            // The tick's second hit-test, against the sprite about to be drawn
            // rather than the one last drawn: whether the next click should
            // reach us is a question about where the art is going to be. A
            // cursor that has just arrived over it must not spend a frame
            // passing clicks to the application underneath.
            let over_sprite = drawn.mask.hit(&sprite, local_x, local_y);
            drawn_last = Some(Drawn {
                rect: sprite,
                animation: frame.animation,
                animation_ms: frame.animation_ms,
            });

            // Click-through returns wherever the sprite is not drawn, and
            // everywhere while the Character is hidden — a Character nobody can
            // see must not swallow a click. The exception is a held Character:
            // a drag that outruns the art would otherwise put the cursor over
            // transparent pixels, hand the button to whatever is underneath,
            // and drop the sprite in the user's hand.
            let ignore = !(presence.visible && (over_sprite || pointer.grabbing()));
            let flipped = ignoring != Some(ignore);

            // Only record the new state once the platform accepted it. Recording
            // it regardless would latch a failed toggle forever, leaving
            // click-through stuck in whichever mode it happened to be in.
            if flipped && window.set_ignore_cursor_events(ignore).is_ok() {
                ignoring = Some(ignore);
            }

            ticks = ticks.wrapping_add(1);
            if tracing && (flipped || ticks % 120 == 0) {
                eprintln!(
                    "hit-test: cursor({:.0},{:.0}) scale(cursor {:.1}, window {:.1}) \
                     -> local({},{}) {} click-through {}{}",
                    cursor.x,
                    cursor.y,
                    cursor_scale,
                    scale,
                    local_x,
                    local_y,
                    if over_sprite { "HIT " } else { "miss" },
                    if ignore { "on" } else { "OFF" },
                    if flipped { "  <- flipped" } else { "" },
                );
            }
        }
    });
}

/// The Character to put on screen: the first package that loads out of every
/// place ai-buddy looks.
///
/// Every rejection is reported before moving on, because a package that was
/// meant to load and did not is exactly what its author needs to hear about. A
/// location that was never a package is not worth a line.
///
/// Finding none stops startup, so the failure names every directory that was
/// searched: that list is the whole of what the reader has to go on.
fn load_character(app: &tauri::AppHandle) -> Result<Cast, String> {
    // The shipped Characters are an app resource, which `tauri-build` copies
    // next to the binary for `cargo run` as well as into a bundle.
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join(BUNDLED_CHARACTERS));

    let search_paths = package::search_paths(bundled);
    let candidates = package::installed(&search_paths);

    for candidate in &candidates {
        let loaded = match package::read(candidate) {
            Ok(loaded) => loaded,
            Err(package::ReadError::NotAPackage(_)) => continue,
            Err(why) => {
                eprintln!("character: {why}");
                continue;
            }
        };

        let name = loaded.character.name.clone();
        match Cast::new(loaded, ALPHA_THRESHOLD) {
            Ok(cast) => {
                eprintln!("character: {name} from {}", candidate.display());
                return Ok(cast);
            }
            // Art the loader accepted and this could not resolve. A rejection
            // like any other: one package with an unreadable frame should not
            // cost the user every Character behind it in the search.
            Err(why) => eprintln!(
                "character: {} could not be drawn: {why}",
                candidate.display()
            ),
        }
    }

    Err(format!(
        "no Character Package loaded. ai-buddy looked in: {}",
        search_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![character])
        .setup(|app| {
            // A companion with no Character has nothing to be, so no Character
            // means no overlay. Reported and exited rather than returned as a
            // setup error: Tauri turns that into a panic the event loop cannot
            // unwind, which buries the one line worth reading under a
            // backtrace.
            let cast = Arc::new(load_character(&app.handle().clone()).unwrap_or_else(|why| {
                eprintln!("character: {why}");
                std::process::exit(1);
            }));
            app.manage(Arc::clone(&cast));

            // Keep ai-buddy out of the Dock and the application switcher. The
            // overlay is furniture, not an app you switch to.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let window = WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::default())
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

            // Click-through until the cursor is proven to be over the sprite. The
            // wrong default swallows a click; this one loses nothing.
            window.set_ignore_cursor_events(true)?;

            // Built before the window is placed rather than after the loop
            // starts: reading which part of a display is usable means asking
            // AppKit, and only the main thread may do that.
            let (source, displays) = platform::window_source(app.handle().clone());
            let start = starting_position(&source.snapshot());

            // The overlay covers the display the Character starts on, and the
            // frame loop moves it to whichever display the Character reaches.
            let home = display_for((start.x, start.y), &displays.read().frames)
                .ok_or("no displays reported")?;
            cover_display(&window, home)?;

            // The sprite size is the idle Animation's, blown up. Animations may
            // declare different frame sizes, so this is what the Character is
            // usually drawn at rather than what it is always drawn at; it is
            // here because scripts/verify-overlay.sh crops a screenshot to it.
            let (sprite_width, sprite_height) =
                cast.draw("idle", 0).map_or((0, 0), |drawn| drawn.art_size);

            eprintln!(
                "overlay: display {:.0}x{:.0} at ({:.0},{:.0}) of {}; character {}; sprite {}x{}",
                home.width,
                home.height,
                home.x,
                home.y,
                displays.read().frames.len(),
                cast.name(),
                sprite_width * SPRITE_SCALE,
                sprite_height * SPRITE_SCALE,
            );

            platform::configure_overlay(&window)?;
            window.show()?;

            // Shared because the hotkey and the frame loop each see half of the
            // answer: the key is pressed on the main thread and the desktop is
            // read on the loop's.
            let rules = Arc::new(Mutex::new(HideRules::default()));
            register_hide_hotkey(app.handle(), Arc::clone(&rules));

            run_frame_loop(
                app.handle().clone(),
                cast,
                source,
                displays,
                rules,
                start,
                home,
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ai-buddy failed to start");
}
