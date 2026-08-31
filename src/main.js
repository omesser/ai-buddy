// The webview draws the Character and owns none of its state. Every tick the
// Rust side sends the Engine's frame; this draws it and remembers nothing
// except the two most recent placements, which is what motion between ticks is
// interpolated across.

import { interpolate } from "./interpolate.js";
import {
  createBubbleMachine,
  wrapText,
  placeBubble,
  CEILING_CLEARANCE,
} from "./bubble.js";

const sprite = document.getElementById("sprite");
const bubble = document.getElementById("bubble");
const thinkingBubble = document.getElementById("thinking-bubble");
const bubbleContent = bubble.querySelector(".bubble-content");

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
// clock and `performance.now()`; worth it if README item 19 ever shows a
// shimmer at the seam under a drag.
let previous = null;
let latest = null;

// Which frame and size were last written to the element. Only the transform
// genuinely changes per display frame; the art and its size change at the
// Engine's rate at most, and writing the same src sixty times a second would
// ask the loader for art it already has.
let drawn = null;

// The bubble decisions live in bubble.js where node can test them; this file
// only supplies the DOM they act on. Showing positions against the newest
// placement; the per-frame follow below keeps a visible bubble tracking a
// walking sprite afterwards.
function currentSpriteRect() {
  return { x: latest.x, y: latest.y, width: latest.width, height: latest.height };
}

function currentDisplayBounds() {
  return { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };
}

function show(element) {
  element.classList.remove("hidden");
  element.classList.add("visible");
  positionBubble(element, currentSpriteRect(), currentDisplayBounds());
}

function hide(element) {
  element.classList.remove("visible");
  const fadeMs = latest?.fade_ms || 0;
  setTimeout(() => element.classList.add("hidden"), fadeMs);
}

const bubbles = createBubbleMachine({
  showSpeech(text) {
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d");
    ctx.font = "14px system-ui, sans-serif";
    bubbleContent.textContent = wrapText(text, 260, ctx.measureText.bind(ctx)).join("\n");
    show(bubble);
  },
  hideSpeech() {
    hide(bubble);
  },
  showThinking() {
    show(thinkingBubble);
  },
  hideThinking() {
    hide(thinkingBubble);
  },
});

function draw(now) {
  requestAnimationFrame(draw);
  if (!latest) {
    return;
  }

  const at = previous ? interpolate(previous, latest, now) : latest;
  const spriteX = Math.round(at.x);
  const spriteY = Math.round(at.y);

  // Whole pixels, so a sprite drawn at an integer scale is not resampled back
  // onto a fractional grid by the compositor. ADR-0006. The art is authored
  // heading right; `scaleX(-1)` mirrors it in place when the Engine says the
  // sprite faces left. main.css sets the center origin that makes it in-place.
  sprite.style.transform = `translate(${spriteX}px, ${spriteY}px) scaleX(${latest.facing})`;

  // The hide rules, carried on every frame rather than announced when they
  // change: a change announced while this file was still fetching its art is a
  // change nobody heard. Writing the same two values again costs nothing and
  // restarts no transition. A rule sends a duration and the hotkey sends zero,
  // so one line covers both the fade and the instant answer.
  sprite.style.transition = `opacity ${latest.fade_ms}ms linear`;
  sprite.style.opacity = latest.visible ? "1" : "0";

  // Bubble visibility follows sprite visibility with same fade
  bubble.style.transition = `opacity ${latest.fade_ms}ms linear`;
  thinkingBubble.style.transition = `opacity ${latest.fade_ms}ms linear`;
  if (!latest.visible) {
    bubble.style.opacity = "0";
    thinkingBubble.style.opacity = "0";
    if (latest.fade_ms === 0) {
      bubbles.hideAllNow();
    }
  } else {
    bubble.style.opacity = "";
    thinkingBubble.style.opacity = "";

    // Reposition visible bubbles to follow walking sprite
    const spriteRect = {
      x: spriteX,
      y: spriteY,
      width: latest.width,
      height: latest.height,
    };
    const displayBounds = {
      x: 0,
      y: 0,
      width: window.innerWidth,
      height: window.innerHeight,
    };

    if (bubble.classList.contains("visible")) {
      positionBubble(bubble, spriteRect, displayBounds);
    }

    if (thinkingBubble.classList.contains("visible")) {
      positionBubble(thinkingBubble, spriteRect, displayBounds);
    }
  }

  bubbles.frame(latest);

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

function positionBubble(element, spriteRect, displayBounds) {
  const bubbleSize = {
    width: element.offsetWidth,
    height: element.offsetHeight,
  };
  const pos = placeBubble(spriteRect, bubbleSize, displayBounds, CEILING_CLEARANCE);
  element.style.left = `${pos.x}px`;
  element.style.top = `${pos.y}px`;
  element.classList.toggle("flipped", pos.flipped);
  element.style.setProperty("--tail-offset", `${pos.tailOffset}px`);
}

async function start() {
  const character = await window.__TAURI__.core.invoke("character");
  art = character.art;
  // The Character Manifest's render_mode: smooth art asks the compositor to
  // filter when scaling, where pixel art (main.css's default) must not.
  if (character.smooth) sprite.style.imageRendering = "auto";

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
      // Dialogue rides exactly one tick, and `latest` keeps only the newest
      // placement: an Engine that ticks faster than the display refreshes
      // overwrites some placements before `draw` ever reads them. The machine
      // latches the pulse here, where every delivery is seen.
      bubbles.event(payload);
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
