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

mod cast;
mod package;
mod platform;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy_core::engine::{Engine, Point};
use ai_buddy_core::input::Pointer;
use ai_buddy_core::overlay::{display_index_for, place_sprite, SpriteRect};
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
    covered: Vec<Rect>,
) {
    thread::spawn(move || {
        let mut engine = Engine::new(start).with_behaviors(cast.behaviors().clone());
        let mut assembler = SnapshotAssembler::new(source);

        // One click-through flag per overlay, `None` until that overlay's first
        // decision so the first tick always applies.
        let mut ignoring: Vec<Option<bool>> = vec![None; covered.len()];

        // The displays the overlays cover, as setup left them. Shared with the
        // main thread, which is the only place that can change what they cover
        // and so the only place that knows when this is true again.
        let covered = Arc::new(Mutex::new(covered));

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

            // Last tick's answer, which is the right one: the art being
            // hit-tested is the art that was last drawn. A Character nobody can
            // see is not there to be pressed, so a click where it would have
            // been reaches the window underneath and pokes nothing.
            let visible = rules.lock().is_ok_and(|rules| rules.presence().visible);

            let pressed_sprite = visible
                && drawn_last.as_ref().is_some_and(|last| {
                    cast.draw(last.animation, last.animation_ms)
                        .is_some_and(|art| art.mask.hit(&last.rect, cursor_at.0, cursor_at.1))
                });
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

            // Placed once, in the space every display shares. Each overlay is
            // handed it in its own coordinates below.
            let sprite = place_sprite(
                (frame.position.x, frame.position.y),
                (width, height),
                SPRITE_SCALE,
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
            let over_sprite = drawn.mask.hit(&sprite, cursor_at.0, cursor_at.1);
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
            let on_overlay =
                display_index_for((cursor_points.x, cursor_points.y), &displays.frames);
            let mut flipped = false;

            for (index, display) in displays.frames.iter().enumerate() {
                let label = overlay_label(index);
                let Some(window) = app.get_webview_window(&label) else {
                    continue; // a display whose overlay has not been built yet
                };
                let local = sprite.in_overlay(*display);

                // Every overlay is told, including the ones the sprite is
                // nowhere near: each draws the part that falls inside it, which
                // is what leaves a Character on a seam whole instead of clipped
                // to one display.
                //
                // Addressed rather than emitted to all, because each overlay
                // is told a different rectangle. src/main.js has to name its
                // own label to match: an untargeted listener hears every emit,
                // addressed elsewhere or not, and would draw whichever
                // rectangle arrived last.
                let _ = window.emit_to(
                    label,
                    FRAME_EVENT,
                    Placement {
                        x: local.x,
                        y: local.y,
                        width,
                        height,
                        animation: frame.animation,
                        frame_index: drawn.index,
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
    let wanted = std::env::var_os(package::CHARACTER_VAR);
    let installed = package::installed(&search_paths);
    let candidates = match &wanted {
        Some(_) => package::named(installed, wanted.as_deref()),
        None => package::preferring(installed, package::DEFAULT_CHARACTER),
    };

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

            // Read before the overlays are built rather than after the loop
            // starts: reading which part of a display is usable means asking
            // AppKit, and only the main thread may do that.
            let (source, displays) = platform::window_source(app.handle().clone());
            let start = starting_position(&source.snapshot());

            // One overlay per display, so a Character straddling a seam is
            // drawn whole. The frame loop keeps the set in step with a desktop
            // that gains or loses a display.
            let covered = displays.read().frames;
            if covered.is_empty() {
                return Err("no displays reported".into());
            }
            place_overlays(app.handle(), &covered)?;

            // The sprite size is the idle Animation's, blown up. Animations may
            // declare different frame sizes, so this is what the Character is
            // usually drawn at rather than what it is always drawn at; it is
            // here because scripts/verify-overlay.sh crops a screenshot to it.
            let (sprite_width, sprite_height) =
                cast.draw("idle", 0).map_or((0, 0), |drawn| drawn.art_size);

            eprintln!(
                "overlay: {} display(s); character {}; sprite {}x{}",
                covered.len(),
                cast.name(),
                sprite_width * SPRITE_SCALE,
                sprite_height * SPRITE_SCALE,
            );

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
                covered,
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ai-buddy failed to start");
}
