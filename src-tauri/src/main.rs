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
mod model;
mod package;
mod platform;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy_core::character::Character;
use ai_buddy_core::director::{
    self, Context, Director, Happened, ModelDirector, Pace, StaticDirector, Wake,
};
use ai_buddy_core::engine::{Engine, Point, State, Verb};
use ai_buddy_core::input::Pointer;
use ai_buddy_core::overlay::{display_index_for, place_sprite, SpriteRect};
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

/// One tick's instruction to the renderer: where to draw the sprite in logical
/// points from the overlay's top-left, and which Animation frame to draw.
///
/// Pushed every tick rather than fetched, so the webview holds no authoritative
/// state — it draws what it was last told and remembers nothing.
#[derive(Clone, Serialize)]
struct Placement<'a> {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// The Animation whose art to draw — the one `Character::draw` resolved
    /// (a variant, or an optional Animation's fallback), which is not always
    /// the name the Engine asked with.
    animation: &'a str,
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

/// What the webview needs of the Character, as Tauri managed state: the art
/// as `data:` URLs, and whether to smooth it when scaling (the Character
/// Manifest's `render_mode`). A struct rather than a bare map so managed
/// state, keyed by type, cannot collide with another map of the same shape.
#[derive(Clone, serde::Serialize)]
struct ArtUrls {
    art: BTreeMap<String, Vec<String>>,
    smooth: bool,
}

/// The Character's art, fetched once by the webview when it loads.
///
/// A command rather than an event: an event emitted during setup would race the
/// webview's own listener, and the art does not change while the app runs.
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
    character: Arc<Character>,
    source: impl WindowSource + Send + 'static,
    displays: platform::DisplayCache,
    rules: Arc<Mutex<HideRules>>,
    start: Point,
    covered: Vec<Rect>,
    director_run: DirectorRun,
) {
    thread::spawn(move || {
        let mut engine = Engine::new(start).with_behaviors(character.behaviors.clone());
        let mut assembler = SnapshotAssembler::new(source);

        // The Static Director, which is the whole of the buddy's life until a
        // Harness is attached: no network, no key, nothing to time out. Seeded
        // from the wall clock so that two runs are not the same afternoon,
        // which is the one thing the Engine's own purity forbids it to do.
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos() as u64);
        let DirectorRun { config, inspect } = director_run;
        let mut director = StaticDirector::new(character.behaviors.clone(), seed);
        let model = config.enabled.then(|| {
            Arc::new(ModelDirector::new(
                model::endpoint().expect("enabled means a key was set"),
                character.behaviors.keys().cloned(),
            ))
        });
        let pending = model::InFlight::new();
        let mut in_flight: Option<Context> = None;
        let mut free_tier = FreeTier::default();
        let activity_source = platform::activity_source();
        let mut recent: Vec<String> = Vec::new();
        let mut since_sense = Duration::ZERO;
        let mut since_wake = Duration::ZERO;
        let mut previous_idle = Duration::MAX;
        let mut since_state = Duration::ZERO;
        let mut last_state: Option<State> = None;
        let mut pace = Pace::with_growth(
            config.ambient_first,
            character.model_base,
            character.model_power,
        );
        let mut since_ambient = Duration::ZERO;
        let mut last_activity: Option<Activity> = None;
        let mut addressed = false;
        let mut happened = Happened::Ambient;

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
        let tracing = env_util::env_flag_is_on("AI_BUDDY_TRACE_HITTEST");

        // Likewise for the Frame: where the sprite is and what it is doing is
        // the loop's only output, and a screenshot cannot say whether it got
        // there by falling.
        let tracing_frames = env_util::env_flag_is_on("AI_BUDDY_TRACE_FRAMES");
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
                    character
                        .draw(last.animation, last.animation_ms)
                        .is_some_and(|art| {
                            art.mask
                                .hit(&last.rect, cursor_at.0, cursor_at.1, last.mirrored)
                        })
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

            // Grab is on every held tick. Only the first tick of a hold is a
            // pick-up; the rest would otherwise wake the session while dragging.
            let grab_started = verbs.iter().any(|verb| matches!(verb, Verb::Grab))
                && last_state != Some(State::Dragged);
            if verbs
                .iter()
                .any(|verb| matches!(verb, Verb::Poke | Verb::Summon | Verb::Throw { .. }))
                || grab_started
            {
                addressed = true;
                happened = if verbs.iter().any(|verb| matches!(verb, Verb::Throw { .. })) {
                    Happened::Throw
                } else if grab_started {
                    Happened::Grab
                } else if verbs.iter().any(|verb| matches!(verb, Verb::Poke)) {
                    Happened::Poke
                } else {
                    Happened::Summon
                };
            }

            // The Director's clock is the same elapsed time the Engine is
            // given, so a loop that stalled wakes it once on the way back
            // rather than in a burst. Firing takes one interval off the clock
            // rather than zeroing it, for the reason `SnapshotAssembler` gives:
            // zeroing throws the overshoot away and stretches every interval by
            // most of a tick.
            let elapsed = Duration::from_millis(u64::from(elapsed_ms));
            since_sense += elapsed;
            since_wake += elapsed;
            since_state += elapsed;
            if !last_activity
                .as_ref()
                .is_some_and(|activity| activity.displays_asleep)
            {
                since_ambient += elapsed;
            }

            let mut proposal = None;
            let arrived = pending.try_take();
            let applied = arrived.is_some();
            if let Some(wake) = arrived {
                let context = in_flight
                    .take()
                    .expect("a started call still has its context");
                if model::tracing() {
                    match &wake {
                        Wake::Proposed(parsed) if !parsed.behavior.is_empty() => eprintln!(
                            "director: parsed {}{}",
                            parsed.behavior,
                            parsed
                                .dialogue
                                .as_deref()
                                .map(|line| format!(" | {line}"))
                                .unwrap_or_default(),
                        ),
                        Wake::Proposed(_) => {}
                        Wake::Failed => eprintln!("director: failed; Static fallback"),
                    }
                }
                proposal = director::fallback(wake, &mut director, &context);
                if model::tracing() {
                    match &proposal {
                        Some(playing) if playing.behavior.is_empty() => {
                            eprintln!(
                                "director: saying {}",
                                playing.dialogue.as_deref().unwrap_or("")
                            );
                        }
                        Some(playing) => eprintln!(
                            "director: playing {}{}",
                            playing.behavior,
                            playing
                                .dialogue
                                .as_deref()
                                .map(|line| format!(" | {line}"))
                                .unwrap_or_default(),
                        ),
                        None => eprintln!("director: nothing to play"),
                    }
                }
            }

            if since_sense >= SENSE_INTERVAL {
                since_sense = since_sense.saturating_sub(SENSE_INTERVAL);
                let activity = free_tier.read(&activity_source, &SystemClock);
                let due = director::due(
                    since_wake,
                    config.wake_every,
                    &activity,
                    previous_idle,
                    since_state,
                    engine.do_not_disturb(),
                );
                previous_idle = activity.idle;

                if due {
                    since_wake = since_wake.saturating_sub(config.wake_every);
                    since_state = Duration::ZERO;
                    // Static keeps the free life going. A session call in
                    // flight is the one exception: do not stack a weight pick
                    // on a proposal that is about to land.
                    if pending.ready() && !applied {
                        proposal = director.propose(&Context {
                            activity: activity.clone(),
                            recent: recent.clone(),
                            personality: character.personality.clone(),
                            state: last_state.unwrap_or(State::Grounded),
                            happened,
                            standing: String::new(),
                        });
                    }
                }
                last_activity = Some(activity);
            }

            let mut world = assembler.assemble(elapsed_ms, cursor_points, verbs);
            world.proposal = proposal;

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

            let frame = engine.tick(&world);
            assembler.poll_fast(frame.riding);

            let became_perched = last_state.is_some()
                && frame.state == State::Perched
                && last_state != Some(State::Perched);
            if became_perched {
                addressed = true;
                happened = Happened::Perch;
            }

            if last_state != Some(frame.state) {
                last_state = Some(frame.state);
                since_state = Duration::ZERO;
            }

            // After the tick so a Throw is already Falling, not still Dragged.
            let reactive_wake =
                if let (Some(model), Some(activity)) = (&model, last_activity.as_ref()) {
                    if director::session_due(
                        addressed,
                        since_ambient,
                        &pace,
                        activity.displays_asleep,
                        engine.do_not_disturb(),
                    ) && pending.ready()
                        && !applied
                    {
                        let context = Context {
                            activity: activity.clone(),
                            recent: recent.clone(),
                            personality: character.personality.clone(),
                            state: frame.state,
                            happened,
                            standing: assembler.standing_on(frame.position),
                        };
                        let was_addressed = addressed;
                        if addressed {
                            pace.after_reactive();
                        } else {
                            pace.after_ambient();
                        }
                        addressed = false;
                        happened = Happened::Ambient;
                        since_ambient = Duration::ZERO;
                        let payload = model.prompt(&context);
                        if let Ok(mut inspect) = inspect.lock() {
                            inspect.last_payload = Some(payload);
                            inspect.wake_secs = pace.wait().as_secs();
                        }
                        pending.start(Arc::clone(model), context.clone());
                        in_flight = Some(context);
                        was_addressed
                    } else {
                        false
                    }
                } else {
                    false
                };

            let thinking = !pending.ready()
                && (reactive_wake
                    || in_flight
                        .as_ref()
                        .is_some_and(|ctx| ctx.happened != Happened::Ambient))
                && !engine.do_not_disturb();

            // What the user has seen is what the Engine played, not what the
            // Director asked for: a proposal the State refuses never reaches
            // the screen, and suppressing it would silence a Behavior nobody
            // watched.
            if let Some(played) = &frame.behavior {
                director::remember(&mut recent, played.clone());
                if tracing_frames {
                    eprintln!("director: {played}");
                }
            }

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
            let Some(drawn) = character.draw(frame.animation, frame.animation_ms) else {
                continue; // a Character with no drawable Animation at all
            };
            let scale = character.scale as i32;
            let (width, height) = (
                drawn.frame_size.0 as i32 * scale,
                drawn.frame_size.1 as i32 * scale,
            );

            // Placed once, in the space every display shares. Each overlay is
            // handed it in its own coordinates below.
            let sprite = place_sprite((frame.position.x, frame.position.y), (width, height), scale);

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
            let mirrored = frame.facing < 0.0;
            let over_sprite = drawn.mask.hit(&sprite, cursor_at.0, cursor_at.1, mirrored);
            drawn_last = Some(Drawn {
                rect: sprite,
                animation: frame.animation,
                animation_ms: frame.animation_ms,
                mirrored,
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
                        animation: drawn.animation,
                        frame_index: drawn.index,
                        visible: presence.visible,
                        fade_ms: presence.fade_ms,
                        facing: frame.facing as i8,
                        dialogue: frame.dialogue.clone(),
                        thinking,
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
fn load_character(app: &tauri::AppHandle) -> Result<Character, String> {
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
            // backtrace.
            let character = Arc::new(load_character(&app.handle().clone()).unwrap_or_else(|why| {
                eprintln!("character: {why}");
                std::process::exit(1);
            }));
            app.manage(ArtUrls {
                art: art_urls(&character),
                smooth: character.smooth,
            });

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

            // The sprite size is the idle Animation's, blown up. Animations may
            // declare different frame sizes, so this is what the Character is
            // usually drawn at rather than what it is always drawn at; it is
            // here because scripts/verify-overlay.sh crops a screenshot to it.
            let (sprite_width, sprite_height) = character
                .draw("idle", 0)
                .map_or((0, 0), |drawn| drawn.frame_size);

            eprintln!(
                "overlay: {} display(s); character {}; sprite {}x{}",
                covered.len(),
                character.name,
                sprite_width as i32 * character.scale as i32,
                sprite_height as i32 * character.scale as i32,
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
            let director_run = DirectorRun { config, inspect };

            run_frame_loop(
                app.handle().clone(),
                character,
                source,
                displays,
                rules,
                start,
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
}
