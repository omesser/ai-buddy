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

mod platform;

use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy::engine::Engine;
use ai_buddy::overlay::{cursor_in_window, AlphaMask, SpriteRect};
use ai_buddy::snapshot::{starting_position, SnapshotAssembler};
use ai_buddy::window_source::WindowSource;
use serde::Serialize;
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

/// The placeholder Character art. Rust reads the pixels to hit-test the cursor;
/// the webview loads the same file to draw it. One file, two readers.
const SPRITE_PNG: &[u8] = include_bytes!("../../src/assets/placeholder-idle.png");
const SPRITE_SRC: &str = "assets/placeholder-idle.png";

/// Nearest-neighbour blow-up, in logical points. ADR-0006 permits integers only.
const SPRITE_SCALE: i32 = 4;

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

/// One tick's instruction to the renderer: where to draw the sprite in logical
/// points from the overlay's top-left, and which Animation frame to draw.
///
/// Pushed every tick rather than fetched, so the webview holds no authoritative
/// state — it draws what it was last told and remembers nothing.
#[derive(Clone, Copy, Serialize)]
struct Placement {
    src: &'static str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    animation: &'static str,
    frame_index: usize,
}

/// The union of every visible display, in logical points.
///
/// The overlay spans all displays so the Character can cross between them
/// without the window moving.
///
/// Logical points, not physical pixels, because each monitor reports its
/// physical geometry against its own scale factor. On a mixed-DPI desktop a
/// 2x built-in display reports an origin already multiplied by 2 while a 1x
/// external display does not, so the two "physical" rectangles share no origin
/// and their union is nonsense. Points are the space macOS composites in and
/// the space the webview draws in, so they are the space to reason in.
fn display_union(
    window: &tauri::WebviewWindow,
) -> Result<(LogicalPosition<f64>, LogicalSize<f64>), String> {
    let monitors = window
        .available_monitors()
        .map_err(|e| format!("cannot enumerate displays: {e}"))?;

    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    for monitor in &monitors {
        let scale = monitor.scale_factor();
        let position = monitor.position();
        let size = monitor.size();

        let left = position.x as f64 / scale;
        let top = position.y as f64 / scale;
        let right = left + size.width as f64 / scale;
        let bottom = top + size.height as f64 / scale;

        bounds = Some(match bounds {
            None => (left, top, right, bottom),
            Some((l, t, r, b)) => (l.min(left), t.min(top), r.max(right), b.max(bottom)),
        });
    }

    let (left, top, right, bottom) = bounds.ok_or("no displays reported")?;
    Ok((
        LogicalPosition::new(left, top),
        LogicalSize::new(right - left, bottom - top),
    ))
}

/// The platform's view of the desktop.
///
/// Windows is stubbed deliberately; see `docs/SPEC.md`. The stub declares no
/// capabilities, so the sprite falls to the screen edges and finds no Perches,
/// which is a supported degraded mode rather than an error.
#[cfg(target_os = "macos")]
fn window_source() -> impl WindowSource {
    ai_buddy::window_source::MacosWindowSource::new()
}

#[cfg(not(target_os = "macos"))]
fn window_source() -> impl WindowSource {
    ai_buddy::window_source::StubWindowSource
}

/// The frame loop: assemble a snapshot, tick the Engine, apply the `Frame`.
///
/// Applying a `Frame` is two things at once, which is why they share a loop.
/// The webview is told where to draw, and the hit-test is told where the sprite
/// now is — the same rectangle, and a stale one would make click-through
/// disagree with what the user can see.
fn run_frame_loop(app: tauri::AppHandle, mask: AlphaMask) {
    thread::spawn(move || {
        let (art_width, art_height) = mask.size();
        let (width, height) = (art_width * SPRITE_SCALE, art_height * SPRITE_SCALE);

        let source = window_source();
        let mut engine = Engine::new(starting_position(&source.snapshot()));
        let mut assembler = SnapshotAssembler::new(source);

        // `None` until the first decision, so the first tick always applies.
        let mut ignoring: Option<bool> = None;

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
            // window's own, so each needs undoing with its own factor. Read per
            // tick rather than cached because displays can be reconfigured while
            // the app runs.
            let cursor_scale = window
                .primary_monitor()
                .ok()
                .flatten()
                .map_or(scale, |monitor| monitor.scale_factor());

            // Wall time since the last tick that reached the Engine, not since
            // the last turn of this loop: a tick that could not read the
            // platform skips without advancing anything, and the time it spent
            // still passed for the sprite. `SnapshotAssembler` caps what it
            // hands the Engine, so a long gap — a skipped read, a suspended
            // process, a slept machine — is absorbed rather than slingshot.
            let elapsed_ms = u32::try_from(last_tick.elapsed().as_millis()).unwrap_or(u32::MAX);
            last_tick = Instant::now();

            let frame = engine.tick(&assembler.assemble(
                elapsed_ms,
                // The Engine works in points across every display, which is the
                // space the cursor reading becomes once its own scale is undone.
                ai_buddy::engine::Point {
                    x: cursor.x / cursor_scale,
                    y: cursor.y / cursor_scale,
                },
            ));

            // The Frame's position is the sprite's contact point in the global
            // space; the webview draws in points from the overlay's top-left,
            // and the art hangs above its feet, centred on them.
            let sprite = SpriteRect {
                x: (frame.position.x - origin.x as f64 / scale).round() as i32 - width / 2,
                y: (frame.position.y - origin.y as f64 / scale).round() as i32 - height,
                scale: SPRITE_SCALE,
            };

            let _ = window.emit(
                FRAME_EVENT,
                Placement {
                    src: SPRITE_SRC,
                    x: sprite.x,
                    y: sprite.y,
                    width,
                    height,
                    animation: frame.animation,
                    frame_index: frame.frame_index,
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
                    frame.frame_index,
                );
            }

            let (local_x, local_y) = cursor_in_window(
                (cursor.x, cursor.y),
                cursor_scale,
                (origin.x as f64, origin.y as f64),
                scale,
            );

            let over_sprite = mask.hit(&sprite, local_x, local_y);
            let ignore = !over_sprite;
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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let mask = AlphaMask::from_png(SPRITE_PNG, ALPHA_THRESHOLD)?;
            let (art_width, art_height) = mask.size();

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

            // ponytail: on a mixed-height desktop the window lands a few points
            // above the union's top, because tao maps a logical top-left through
            // the primary display's height rather than the union's. Harmless
            // here — the hit-test derives local coordinates from the window's
            // real position — but it leaves a thin strip of the taller display
            // uncovered. #4 owns clamping physics to the union and should fix
            // the origin properly.
            let (position, size) = display_union(&window)?;
            window.set_position(position)?;
            window.set_size(size)?;

            eprintln!(
                "overlay: union {:.0}x{:.0} at ({:.0},{:.0}); sprite {}x{}",
                size.width,
                size.height,
                position.x,
                position.y,
                art_width * SPRITE_SCALE,
                art_height * SPRITE_SCALE,
            );

            platform::configure_overlay(&window)?;
            window.show()?;

            run_frame_loop(app.handle().clone(), mask);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ai-buddy failed to start");
}
