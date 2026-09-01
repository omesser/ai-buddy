// The webview draws the Characters and owns none of their state. Every tick the
// Rust side sends one placement per Instance; this draws them and remembers
// nothing except the two most recent placements of each, which is what motion
// between ticks is interpolated across.
//
// One element per Instance, created the first time an id is seen and removed
// when it stops arriving. The Rust side sends the whole set every tick, so the
// list is also the answer to which Instances still exist — a dismissed buddy is
// one that is no longer mentioned.

import { interpolate } from "./interpolate.js";
import {
  createBubbleMachine,
  wrapText,
  placeBubble,
  CEILING_CLEARANCE,
} from "./bubble.js";

const stage = document.getElementById("stage");

// Every Character's art as data: URLs, keyed by Character name and fetched
// once. Art, not state, and one entry however many Instances draw from it.
let characters = {};

// One view per Instance, keyed by its id.
const views = new Map();

function currentDisplayBounds() {
  return { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };
}

// Everything one Instance's sprite needs to be drawn: its elements, the two
// placements motion is interpolated across, and its own bubble machine.
//
// Per Instance rather than shared, because two buddies speak on their own
// schedules: one bubble machine between them would hand a line meant for one to
// whichever drew last, and a thinking indicator started by one would be
// cancelled by the other's silence.
function createView(id) {
  const sprite = document.createElement("img");
  sprite.className = "sprite";
  sprite.alt = "";
  sprite.dataset.instance = id;

  const bubble = document.createElement("div");
  bubble.className = "bubble";
  bubble.dataset.instance = id;
  const bubbleContent = document.createElement("div");
  bubbleContent.className = "bubble-content";
  const dots = document.createElement("div");
  dots.className = "thinking-dots";
  for (let i = 0; i < 3; i += 1) {
    dots.appendChild(document.createElement("span"));
  }
  bubble.append(bubbleContent, dots);

  // Both, in one call, because a sprite and its bubble are stacked by the
  // z-index written every tick rather than by the order they were added. Append
  // order would put whichever Instance was seen first at the back regardless of
  // where the Rust side draws it, and a dismissed id reappearing would jump to
  // the front.
  stage.append(bubble, sprite);

  const view = {
    sprite,
    bubble,
    bubbleContent,
    // The two most recent placements and when each arrived. Drawing the latest
    // one the instant it lands would put the sprite wherever the Engine's tick
    // happened to fall relative to the display's refresh, which is a stutter
    // rather than motion. Drawing one sample behind, interpolated, buys
    // continuous motion; see interpolate.js for what that costs and why it is
    // worth it.
    //
    // The hit-test runs against the unlagged position, so the click-through
    // rectangle leads what is on screen by one sample — a pixel or two at
    // walking speed, and always in the direction of travel.
    //
    // ponytail: every overlay interpolates on its own clock — its own arrival
    // times and its own frame phase — so the two halves of a sprite on a seam
    // can round a point apart while it is moving. At rest they cannot, there
    // being nothing to interpolate. The upgrade is to carry the Engine's own
    // timestamp on the frame and solve for it, which needs a per-webview offset
    // between that clock and `performance.now()`; worth it if README item 19
    // ever shows a shimmer at the seam under a drag.
    previous: null,
    latest: null,
    // Which frame and size were last written to the element. Only the transform
    // genuinely changes per display frame; the art and its size change at the
    // Engine's rate at most, and writing the same src sixty times a second
    // would ask the loader for art it already has.
    drawn: null,
    // Which Character's art this view is drawing, so a change of art also
    // changes how it is filtered when scaled.
    character: null,
  };

  function spriteRect() {
    return {
      x: view.latest.x,
      y: view.latest.y,
      width: view.latest.width,
      height: view.latest.height,
    };
  }

  // bubble.js decides that speech and the indicator never coincide; one element
  // in one of two modes is what makes that true of the pixels rather than only
  // of the decision, so a hand-off is a content swap and not a race between
  // fades.
  //
  // Nothing but opacity gates it, and a second class latched by a timer is the
  // trap to avoid: the only duration to hand is `fade_ms`, which measures the
  // Character's presence fade and rides every frame long after the rule that
  // set it. Keying a bubble's exit on that kept a dismissed one painted for
  // half a second — longer than the grace before the indicator arrives.
  function show(mode) {
    bubble.dataset.mode = mode;
    bubble.classList.add("visible");
    positionBubble(view, spriteRect(), currentDisplayBounds());
  }

  function hide() {
    bubble.classList.remove("visible");
  }

  view.bubbles = createBubbleMachine({
    showSpeech(text) {
      const canvas = document.createElement("canvas");
      const ctx = canvas.getContext("2d");
      ctx.font = "14px system-ui, sans-serif";
      view.bubbleContent.textContent = wrapText(text, 260, ctx.measureText.bind(ctx)).join("\n");
      show("speech");
    },
    hideSpeech: hide,
    showThinking() {
      show("thinking");
    },
    hideThinking: hide,
  });

  return view;
}

function positionBubble(view, spriteRect, displayBounds) {
  const bubbleSize = { width: view.bubble.offsetWidth, height: view.bubble.offsetHeight };
  const pos = placeBubble(spriteRect, bubbleSize, displayBounds, CEILING_CLEARANCE);
  view.bubble.style.left = `${pos.x}px`;
  view.bubble.style.top = `${pos.y}px`;
  view.bubble.classList.toggle("flipped", pos.flipped);
  view.bubble.style.setProperty("--tail-offset", `${pos.tailOffset}px`);
}

// An Instance that stopped arriving was dismissed. Its elements go with it, and
// its bubble timers are cancelled first: a scheduled callback holding a removed
// element would keep the view alive to no visible end.
function removeView(id) {
  const view = views.get(id);
  if (!view) return;
  view.bubbles.hideAllNow();
  view.sprite.remove();
  view.bubble.remove();
  views.delete(id);
}

function drawView(view, now) {
  const { latest } = view;
  const at = view.previous ? interpolate(view.previous, latest, now) : latest;
  const spriteX = Math.round(at.x);
  const spriteY = Math.round(at.y);

  // Whole pixels, so a sprite drawn at an integer scale is not resampled back
  // onto a fractional grid by the compositor. ADR-0006. The art is authored
  // heading right; `scaleX(-1)` mirrors it in place when the Engine says the
  // sprite faces left. main.css sets the center origin that makes it in-place.
  view.sprite.style.transform = `translate(${spriteX}px, ${spriteY}px) scaleX(${latest.facing})`;

  // The hide rules, carried on every frame rather than announced when they
  // change: a change announced while this file was still fetching its art is a
  // change nobody heard. Writing the same two values again costs nothing and
  // restarts no transition. A rule sends a duration and the hotkey sends zero,
  // so one line covers both the fade and the instant answer. Every Instance is
  // told the same answer — the rules are about the desktop, not about a sprite.
  view.sprite.style.transition = `opacity ${latest.fade_ms}ms linear`;
  view.sprite.style.opacity = latest.visible ? "1" : "0";

  // Bubble visibility follows sprite visibility with same fade
  view.bubble.style.transition = `opacity ${latest.fade_ms}ms linear`;
  if (!latest.visible) {
    view.bubble.style.opacity = "0";
    if (latest.fade_ms === 0) {
      view.bubbles.hideAllNow();
    }
  } else {
    view.bubble.style.opacity = "";

    // Reposition a visible bubble to follow walking sprite
    if (view.bubble.classList.contains("visible")) {
      positionBubble(
        view,
        { x: spriteX, y: spriteY, width: latest.width, height: latest.height },
        currentDisplayBounds(),
      );
    }
  }

  view.bubbles.frame(latest);

  const placement = `${latest.character} ${latest.animation}#${latest.frame_index} ${latest.width}x${latest.height}`;
  if (placement === view.drawn) {
    return;
  }
  view.drawn = placement;

  const art = characters[latest.character];
  if (view.character !== latest.character) {
    view.character = latest.character;
    // The Character Manifest's render_mode: smooth art asks the compositor to
    // filter when scaling, where pixel art (main.css's default) must not.
    view.sprite.style.imageRendering = art?.smooth ? "auto" : "";
  }

  const src = art?.art[latest.animation]?.[latest.frame_index];
  if (src) {
    view.sprite.src = src;
  }
  view.sprite.style.width = `${latest.width}px`;
  view.sprite.style.height = `${latest.height}px`;
  view.sprite.dataset.animation = latest.animation;
  view.sprite.dataset.frameIndex = latest.frame_index;
  view.sprite.style.visibility = "visible";
}

function draw(now) {
  requestAnimationFrame(draw);
  for (const view of views.values()) {
    if (view.latest) {
      drawView(view, now);
    }
  }
}

async function start() {
  characters = (await window.__TAURI__.core.invoke("character")).characters;

  // There is one overlay per display and each is told where every sprite is in
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
      const arrived = new Set();

      payload.sprites.forEach((sprite, index) => {
        arrived.add(sprite.id);
        let view = views.get(sprite.id);
        if (!view) {
          view = createView(sprite.id);
          views.set(sprite.id, view);
        }

        // Stacked in the order the Rust side sent them, so the last sprite in
        // the list is the one in front. `input::press_target` picks the last
        // hit for exactly that reason: it takes the sprite the user can see
        // under the cursor. Written here rather than left to append order,
        // which is what makes the two sides agree instead of coincide.
        //
        // A sprite sits in front of its own bubble at every level, so speech
        // near the top of a display is never painted over the Character
        // speaking it.
        view.bubble.style.zIndex = `${index * 2}`;
        view.sprite.style.zIndex = `${index * 2 + 1}`;

        view.previous = view.latest;
        view.latest = {
          ...sprite,
          visible: payload.visible,
          fade_ms: payload.fade_ms,
          at: performance.now(),
        };
        // Dialogue rides exactly one tick, and `latest` keeps only the newest
        // placement: an Engine that ticks faster than the display refreshes
        // overwrites some placements before `draw` ever reads them. The machine
        // latches the pulse here, where every delivery is seen.
        view.bubbles.event(sprite);
      });

      // An id that stopped arriving is an Instance that was dismissed, so its
      // elements go. The Rust side must therefore never drop a live Instance
      // from the list to mean anything else — absence is this, and a sprite
      // taken away here loses its bubble and its interpolation with it.
      for (const id of [...views.keys()]) {
        if (!arrived.has(id)) {
          removeView(id);
        }
      }
    },
    { target: overlay.label },
  );

  requestAnimationFrame(draw);

  // The overlay only receives these while click-through is off — over the
  // art. The Rust side's session button poll has been seen to miss that
  // press, which is how a click on the sprite produced no Poke.
  // Last write wins: two in-flight invokes can complete out of order, and
  // an up that lands before its down would leave the latch stuck true.
  const reportPrimary = (() => {
    let inflight = false;
    let queued = null;
    const send = (down) => {
      inflight = true;
      window.__TAURI__.core
        .invoke("overlay_primary", { down })
        .catch((err) => {
          console.error("overlay_primary", err);
        })
        .finally(() => {
          inflight = false;
          if (queued !== null) {
            const next = queued;
            queued = null;
            send(next);
          }
        });
    };
    return (down) => {
      if (inflight) {
        queued = down;
        return;
      }
      send(down);
    };
  })();
  document.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.target.setPointerCapture?.(event.pointerId);
    reportPrimary(true);
  });
  document.addEventListener("pointerup", (event) => {
    if (event.button !== 0) return;
    reportPrimary(false);
  });
  document.addEventListener("pointercancel", () => {
    reportPrimary(false);
  });
}

start().catch((err) => {
  // No art or no frames means nothing to draw and nothing to hit-test, so say
  // so loudly rather than showing an empty overlay that looks like a hung app.
  console.error("ai-buddy could not draw the Character:", err);
});
