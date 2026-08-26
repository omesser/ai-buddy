//! ai-buddy's overlay shell.
//!
//! One transparent, always-on-top window renders the Character. Click-through on
//! macOS is per-window rather than per-pixel, so a screen-sized transparent
//! window would swallow every click. The shell therefore tracks the cursor and
//! toggles ignore-mouse-events by hit-testing the sprite's alpha, which is what
//! makes the overlay feel like a sprite on the desktop instead of a sheet of
//! glass over it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;

use std::thread;
use std::time::Duration;

use ai_buddy::overlay::{cursor_in_window, AlphaMask, SpriteRect};
use serde::Serialize;
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

/// The placeholder Character art. Rust reads the pixels to hit-test the cursor;
/// the webview loads the same file to draw it. One file, two readers.
const SPRITE_PNG: &[u8] = include_bytes!("../../src/assets/placeholder-idle.png");
const SPRITE_SRC: &str = "assets/placeholder-idle.png";

/// Nearest-neighbour blow-up, in logical points. ADR-0006 permits integers only.
const SPRITE_SCALE: i32 = 4;

/// Alpha at or above this counts as drawn. See `AlphaMask::from_png`.
const ALPHA_THRESHOLD: u8 = 128;

/// Roughly 60Hz. Polling is not laziness: a click-through window receives no
/// mouse events at all, so the webview cannot tell us when the cursor returns.
/// Something outside the window has to ask where the cursor is.
const CURSOR_POLL: Duration = Duration::from_millis(16);

const OVERLAY_LABEL: &str = "overlay";

/// Where the webview should draw the sprite, in logical points.
#[derive(Clone, Copy, Serialize)]
struct Placement {
    src: &'static str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

struct Overlay {
    mask: AlphaMask,
    sprite: SpriteRect,
    placement: Placement,
}

/// The webview holds no authoritative state, so it asks where to draw.
#[tauri::command]
fn placement(overlay: tauri::State<'_, Overlay>) -> Placement {
    overlay.placement
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

/// Where the sprite starts, in logical points from the overlay's top-left.
///
/// ponytail: an env override because verifying click-through on a second display
/// needs the sprite over there, and nothing can move it until Grab lands in #6.
/// Delete this once the sprite can be dragged.
fn starting_position() -> (i32, i32) {
    let parse = || {
        let raw = std::env::var("AI_BUDDY_SPRITE_POS").ok()?;
        let (x, y) = raw.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    };
    parse().unwrap_or((160, 160))
}

/// Toggle click-through as the cursor crosses the sprite's drawn pixels.
fn track_cursor(app: tauri::AppHandle) {
    thread::spawn(move || {
        // `None` until the first decision, so the first poll always applies.
        let mut ignoring: Option<bool> = None;

        // Click-through is invisible: nothing on screen says whether the overlay
        // is currently swallowing clicks or passing them on. This trace is the
        // only way to watch the decision without a human clicking. Off unless
        // asked for; see scripts/verify-overlay.sh.
        let tracing = std::env::var_os("AI_BUDDY_TRACE_HITTEST").is_some();
        let mut ticks: u32 = 0;

        loop {
            thread::sleep(CURSOR_POLL);

            let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
                return; // window is gone; so is the reason to poll
            };
            let overlay = app.state::<Overlay>();

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

            let (local_x, local_y) = cursor_in_window(
                (cursor.x, cursor.y),
                cursor_scale,
                (origin.x as f64, origin.y as f64),
                scale,
            );

            let over_sprite = overlay.mask.hit(&overlay.sprite, local_x, local_y);
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
        .invoke_handler(tauri::generate_handler![placement])
        .setup(|app| {
            let mask = AlphaMask::from_png(SPRITE_PNG, ALPHA_THRESHOLD)?;
            let (art_width, art_height) = mask.size();
            let (x, y) = starting_position();

            let placement = Placement {
                src: SPRITE_SRC,
                x,
                y,
                width: art_width * SPRITE_SCALE,
                height: art_height * SPRITE_SCALE,
            };
            app.manage(Overlay {
                sprite: SpriteRect { x, y, scale: SPRITE_SCALE },
                placement,
                mask,
            });

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

            let (position, size) = display_union(&window)?;
            window.set_position(position)?;
            window.set_size(size)?;

            // ponytail: on a mixed-height desktop the window lands a few points
            // above the union's top, because tao maps a logical top-left through
            // the primary display's height rather than the union's. Harmless
            // here — the hit-test derives local coordinates from the window's
            // real position — but it leaves a thin strip of the taller display
            // uncovered. #4 owns clamping physics to the union and should fix
            // the origin properly.
            eprintln!(
                "overlay: union {:.0}x{:.0} at ({:.0},{:.0}); sprite {}x{} at ({},{})",
                size.width,
                size.height,
                position.x,
                position.y,
                placement.width,
                placement.height,
                placement.x,
                placement.y,
            );

            platform::configure_overlay(&window)?;
            window.show()?;

            track_cursor(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ai-buddy failed to start");
}
