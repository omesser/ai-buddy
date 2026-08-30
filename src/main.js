// The webview draws the Character and owns none of its state. Every tick the
// Rust side sends the Engine's frame; this draws it and remembers nothing
// except the two most recent placements, which is what motion between ticks is
// interpolated across.

import { interpolate } from "./interpolate.js";

const sprite = document.getElementById("sprite");

// Every Animation's frames as data: URLs, fetched once. Art, not state.
let art = {};

// The two most recent placements and when each arrived. Drawing the latest one
// the instant it lands would put the sprite wherever the Engine's tick happened
// to fall relative to the display's refresh, which is a stutter rather than
// motion. Drawing one sample behind, interpolated, buys continuous motion; see
// interpolate.js for what that costs and why it is worth it.
//
// The hit-test runs against the unlagged position, so the click-through
// rectangle leads what is on screen by one sample — a pixel or two at walking
// speed, and always in the direction of travel.
//
// ponytail: every overlay interpolates on its own clock — its own arrival
// times and its own frame phase — so the two halves of a sprite on a seam can
// round a point apart while it is moving. At rest they cannot, there being
// nothing to interpolate. The upgrade is to carry the Engine's own timestamp
// on the frame and solve for it, which needs a per-webview offset between that
// clock and `performance.now()`; worth it if README item 13 ever shows a
// shimmer at the seam under a drag.
let previous = null;
let latest = null;

// Which frame and size were last written to the element. Only the transform
// genuinely changes per display frame; the art and its size change at the
// Engine's rate at most, and writing the same src sixty times a second would
// ask the loader for art it already has.
let drawn = null;

function draw(now) {
  requestAnimationFrame(draw);
  if (!latest) {
    return;
  }

  const at = previous ? interpolate(previous, latest, now) : latest;
  // Whole pixels, so a sprite drawn at an integer scale is not resampled back
  // onto a fractional grid by the compositor. ADR-0006.
  sprite.style.transform = `translate(${Math.round(at.x)}px, ${Math.round(at.y)}px)`;

  const placement = `${latest.animation}#${latest.frame_index} ${latest.width}x${latest.height}`;
  if (placement === drawn) {
    return;
  }
  drawn = placement;

  const src = art[latest.animation]?.[latest.frame_index];
  if (src) {
    sprite.src = src;
  }
  sprite.style.width = `${latest.width}px`;
  sprite.style.height = `${latest.height}px`;
  sprite.dataset.animation = latest.animation;
  sprite.dataset.frameIndex = latest.frame_index;
  sprite.style.visibility = "visible";
}

async function start() {
  art = await window.__TAURI__.core.invoke("character");

  // There is one overlay per display and each is told where the sprite is in
  // its own coordinates, so this asks for the frames addressed to this window
  // and no others. Not optional, and not for the reason it looks like: a
  // listener registered with no target is an `Any` listener, and tauri hands an
  // `Any` listener every emit, addressed elsewhere or not. Without this each
  // overlay would also hear its neighbours' rectangles and draw whichever
  // arrived last, so every display would show the same display's half.
  const overlay = window.__TAURI__.webviewWindow.getCurrentWebviewWindow();

  await window.__TAURI__.event.listen(
    "frame",
    ({ payload }) => {
      previous = latest;
      latest = { ...payload, at: performance.now() };
    },
    { target: overlay.label },
  );

  requestAnimationFrame(draw);
}

start().catch((err) => {
  // No art or no frames means nothing to draw and nothing to hit-test, so say
  // so loudly rather than showing an empty overlay that looks like a hung app.
  console.error("ai-buddy could not draw the Character:", err);
});
