// Run with `node --test tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  bubbleDuration,
  wrapText,
  placeBubble,
  createBubbleMachine,
  forOverlay,
  CEILING_CLEARANCE,
  THINKING_GRACE_MS,
  THINKING_MIN_HOLD_MS,
} from "../src/bubble.js";

const testMeasureFn = (text) => ({ width: text.length * 8 });

test("bubble duration is 900ms + 55ms per character, clamped to 2-8s", () => {
  assert.equal(bubbleDuration("hi"), 2000, "min clamp");
  assert.equal(bubbleDuration("hello"), 2000, "still at min");
  // "hello there" = 11 chars = 900 + 605 = 1505ms, clamped to 2000ms min
  assert.equal(bubbleDuration("hello there"), 2000, "short text clamped to min");
  const medium = "a".repeat(30);
  assert.equal(bubbleDuration(medium), 900 + 55 * 30, "base + per-char, no clamp");

  const long = "a".repeat(200);
  assert.equal(bubbleDuration(long), 8000, "max clamp");
});

test("wrap text at max width", () => {
  const short = "hi";
  const wrapped = wrapText(short, 260, testMeasureFn);
  assert.equal(wrapped.length, 1);
  assert.equal(wrapped[0], "hi");
});

test("long text wraps at word boundaries", () => {
  const text = "The quick brown fox jumps over the lazy dog";
  const wrapped = wrapText(text, 100, testMeasureFn);
  assert.ok(wrapped.length > 1, "text should wrap");
  assert.ok(wrapped.every(line => line.length > 0), "no empty lines");
});

test("text truncates with ellipsis past 6 lines", () => {
  const manyLines = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8";
  const wrapped = wrapText(manyLines, 260, testMeasureFn);
  assert.ok(wrapped.length <= 6, "truncated to 6 lines");
  if (wrapped.length === 6) {
    assert.ok(wrapped[5].endsWith("…"), "last line has ellipsis");
  }
});

test("bubble placement stays above sprite by default", () => {
  const spriteRect = { x: 100, y: 400, width: 64, height: 64 };
  const bubbleSize = { width: 200, height: 100 };
  const displayBounds = { x: 0, y: 0, width: 1000, height: 800 };

  const pos = placeBubble(spriteRect, bubbleSize, displayBounds, CEILING_CLEARANCE);

  assert.ok(pos.y < spriteRect.y, "bubble is above sprite");
  assert.ok(pos.x >= displayBounds.x, "bubble is within display left");
  assert.ok(pos.x + bubbleSize.width <= displayBounds.x + displayBounds.width, "bubble is within display right");
  assert.equal(pos.flipped, false, "not flipped when above");
  assert.equal(pos.tailOffset, 0, "tail centered when bubble not clamped");
});

test("bubble flips below sprite when near ceiling", () => {
  const spriteRect = { x: 100, y: 50, width: 64, height: 64 };
  const bubbleSize = { width: 200, height: 100 };
  const displayBounds = { x: 0, y: 0, width: 1000, height: 800 };

  const pos = placeBubble(spriteRect, bubbleSize, displayBounds, CEILING_CLEARANCE);

  assert.ok(pos.y > spriteRect.y + spriteRect.height, "bubble flips below when near ceiling");
  assert.equal(pos.flipped, true, "flipped flag is true");
});

test("bubble slides horizontally at display edges", () => {
  const spriteNearLeftEdge = { x: 10, y: 400, width: 64, height: 64 };
  const bubbleSize = { width: 200, height: 100 };
  const displayBounds = { x: 0, y: 0, width: 1000, height: 800 };

  const pos = placeBubble(spriteNearLeftEdge, bubbleSize, displayBounds, CEILING_CLEARANCE);

  assert.ok(pos.x >= displayBounds.x, "bubble clamped to left edge");
  assert.ok(pos.x + bubbleSize.width <= displayBounds.x + displayBounds.width, "bubble within right bound");
  assert.equal(typeof pos.flipped, "boolean", "flipped flag is present");
  const spriteCenterX = spriteNearLeftEdge.x + spriteNearLeftEdge.width / 2;
  const bubbleCenterX = pos.x + bubbleSize.width / 2;
  assert.equal(pos.tailOffset, spriteCenterX - bubbleCenterX, "tail offset points to sprite");
});

test("bubble stays whole across display seam", () => {
  const spriteAtSeam = { x: 995, y: 400, width: 64, height: 64 };
  const bubbleSize = { width: 200, height: 100 };
  const displayBounds = { x: 0, y: 0, width: 1000, height: 800 };

  const pos = placeBubble(spriteAtSeam, bubbleSize, displayBounds, CEILING_CLEARANCE);

  assert.ok(pos.x >= displayBounds.x, "bubble not past left edge");
  assert.ok(pos.x + bubbleSize.width <= displayBounds.x + displayBounds.width, "bubble not past right edge");
  assert.equal(typeof pos.flipped, "boolean", "flipped flag is present");
});

// --- The bubble machine: what shows and hides, driven by fake timers. ---

function machineHarness() {
  let now = 0;
  let nextId = 1;
  const timers = new Map();
  const calls = [];
  // main.js draws both bubbles into one element in one of two modes, so the
  // harness models that element rather than two booleans: speech strictly
  // wins, and on one surface the two cannot coincide at all. What one surface
  // does make possible is a hide landing after the show that replaced it,
  // which reads here as a surface that went blank.
  let surface = null;
  const machine = createBubbleMachine({
    showSpeech(text) {
      surface = "speech";
      calls.push(`showSpeech:${text}`);
    },
    hideSpeech() {
      surface = null;
      calls.push("hideSpeech");
    },
    showThinking() {
      surface = "thinking";
      calls.push("showThinking");
    },
    hideThinking() {
      surface = null;
      calls.push("hideThinking");
    },
    schedule(fn, ms) {
      const id = nextId++;
      timers.set(id, { fn, at: now + ms });
      return id;
    },
    cancel(id) {
      timers.delete(id);
    },
  });
  // Steps timer by timer so a callback that schedules (the grace arming the
  // min-hold) sees the clock at its own fire time, the way real timers do.
  const advance = (ms) => {
    const target = now + ms;
    for (;;) {
      const due = [...timers.entries()]
        .filter(([, timer]) => timer.at <= target)
        .sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      const [id, timer] = due;
      timers.delete(id);
      now = timer.at;
      timer.fn();
    }
    now = target;
  };
  const placement = (overrides) => ({
    dialogue: null,
    thinking: false,
    visible: true,
    ...overrides,
  });
  return { machine, calls, advance, placement, surface: () => surface };
}

test("the bubble's one surface is never taken back off the wrong one", () => {
  const { machine, advance, placement, surface } = machineHarness();

  machine.frame(placement({ thinking: true }));
  advance(THINKING_GRACE_MS);
  assert.equal(surface(), "thinking", "grace elapsed, indicator up");

  const reply = placement({ dialogue: "hello" });
  machine.event(reply);
  machine.frame(reply);
  assert.equal(surface(), "speech", "the indicator's hide lands before the reply's show");

  // A second turn starts while the reply is still being read. It may not take
  // the surface, and the hide that ends it may not arrive early.
  machine.frame(placement({ thinking: true }));
  advance(bubbleDuration("hello") - 1);
  assert.equal(surface(), "speech", "the reply holds the surface for its reading time");

  advance(1);
  assert.equal(surface(), null, "reading time up");

  advance(THINKING_GRACE_MS);
  assert.equal(surface(), "thinking", "the turn still in flight gets the surface back");
});

test("a reply hides the thinking indicator the same frame its bubble shows", () => {
  const { machine, calls, advance, placement } = machineHarness();

  const thinkingTick = placement({ thinking: true });
  machine.event(thinkingTick);
  machine.frame(thinkingTick);
  advance(THINKING_GRACE_MS);
  assert.deepEqual(calls, ["showThinking"], "grace elapsed, indicator up");

  // The reply lands while the min-hold is still pending: the hold is for
  // silent endings and must not delay the answer.
  const reply = placement({ dialogue: "hello" });
  machine.event(reply);
  machine.frame(reply);
  assert.deepEqual(
    calls,
    ["showThinking", "hideThinking", "showSpeech:hello"],
    "indicator gone before the speech bubble shows, no timers involved",
  );
});

test("a dialogue pulse overwritten before the next drawn frame still shows", () => {
  const { machine, calls, placement } = machineHarness();

  // The Engine ticks faster than the display refreshes: the dialogue tick
  // and its successor both arrive before draw samples the newest placement.
  machine.event(placement({ dialogue: "missed me?" }));
  machine.event(placement({}));
  machine.frame(placement({}));

  assert.deepEqual(calls, ["showSpeech:missed me?"], "the pulse was latched");
});

test("a silent ending within the min-hold clears at the hold, not before", () => {
  const { machine, calls, advance, placement } = machineHarness();

  const thinkingTick = placement({ thinking: true });
  machine.frame(thinkingTick);
  advance(THINKING_GRACE_MS);
  assert.deepEqual(calls, ["showThinking"]);

  machine.frame(placement({}));
  assert.deepEqual(calls, ["showThinking"], "held: no flicker on a fast silent end");

  advance(THINKING_MIN_HOLD_MS);
  assert.deepEqual(calls, ["showThinking", "hideThinking"], "cleared at the hold");
});

test("a silent ending after the min-hold clears immediately", () => {
  const { machine, calls, advance, placement } = machineHarness();

  machine.frame(placement({ thinking: true }));
  advance(THINKING_GRACE_MS + THINKING_MIN_HOLD_MS);
  assert.deepEqual(calls, ["showThinking"], "still thinking at hold expiry");

  machine.frame(placement({}));
  assert.deepEqual(calls, ["showThinking", "hideThinking"]);
});

test("a reply faster than the grace never shows the indicator", () => {
  const { machine, calls, advance, placement } = machineHarness();

  machine.frame(placement({ thinking: true }));
  const reply = placement({ dialogue: "quick" });
  machine.event(reply);
  machine.frame(reply);
  advance(THINKING_GRACE_MS + THINKING_MIN_HOLD_MS);

  assert.deepEqual(calls, ["showSpeech:quick"], "no flash of the indicator");
});

test("the speech bubble hides itself after its reading time", () => {
  const { machine, calls, advance, placement } = machineHarness();

  const reply = placement({ dialogue: "hi" });
  machine.event(reply);
  machine.frame(reply);
  advance(bubbleDuration("hi"));

  assert.deepEqual(calls, ["showSpeech:hi", "hideSpeech"]);
});

test("a new turn waits behind a displayed reply, indicator only after it hides", () => {
  const { machine, calls, advance, placement } = machineHarness();

  const reply = placement({ dialogue: "hi" });
  machine.event(reply);
  machine.frame(reply);
  assert.deepEqual(calls, ["showSpeech:hi"]);

  // A second poke starts a new turn while the reply is still on screen: the
  // indicator must not appear over it, however long the turn runs.
  machine.frame(placement({ thinking: true }));
  advance(THINKING_GRACE_MS + THINKING_MIN_HOLD_MS);
  assert.deepEqual(calls, ["showSpeech:hi"], "nothing shows over the reply");

  // Reading time up: the reply hides, and the still-running turn starts its
  // grace from this moment.
  advance(bubbleDuration("hi") - THINKING_GRACE_MS - THINKING_MIN_HOLD_MS);
  assert.deepEqual(calls, ["showSpeech:hi", "hideSpeech"]);

  advance(THINKING_GRACE_MS);
  assert.deepEqual(calls, ["showSpeech:hi", "hideSpeech", "showThinking"]);
});

test("a reply landing in the post-speech grace never flashes the indicator", () => {
  const { machine, calls, advance, placement } = machineHarness();

  const first = placement({ dialogue: "hi" });
  machine.event(first);
  machine.frame(first);
  machine.frame(placement({ thinking: true }));
  advance(bubbleDuration("hi"));
  assert.deepEqual(calls, ["showSpeech:hi", "hideSpeech"], "grace just restarted");

  const second = placement({ dialogue: "again" });
  machine.event(second);
  machine.frame(second);
  advance(THINKING_GRACE_MS + THINKING_MIN_HOLD_MS);

  assert.deepEqual(
    calls,
    ["showSpeech:hi", "hideSpeech", "showSpeech:again"],
    "the indicator never appeared",
  );
});

// --- #178: one overlay owns the bubble; the rest draw the art only. ---

test("a placement this overlay does not own carries no bubble", () => {
  const spoken = { dialogue: "Yare yare daze.", thinking: true, bubble: true, x: 1 };
  assert.equal(forOverlay(spoken), spoken, "the owner sees it untouched");

  const elsewhere = forOverlay({ ...spoken, bubble: false });
  assert.equal(elsewhere.dialogue, null, "no line to latch on the wrong display");
  assert.equal(elsewhere.thinking, false, "no thinking to arm on the wrong display");
  assert.equal(elsewhere.x, 1, "everything the art needs is left alone");
});

test("a losing overlay never arms the indicator off a thinking it does not own", () => {
  const { machine, advance, placement, surface } = machineHarness();

  machine.frame(forOverlay(placement({ thinking: true, bubble: false })));
  advance(THINKING_GRACE_MS + THINKING_MIN_HOLD_MS);
  assert.equal(surface(), null, "grace never armed: this display is not the owner");

  machine.frame(forOverlay(placement({ thinking: true, bubble: true })));
  advance(THINKING_GRACE_MS);
  assert.equal(surface(), "thinking", "the same frame, owned, arms it");
});

test("a line crossing the seam hides on the old display before it shows on the new", () => {
  // Two overlays, two machines: the shell hands the line to the owner, and on a
  // crossing says it again to the new one (`carry_line`), while the old one
  // hides on the tick it loses ownership — the way main.js does on `!bubble`.
  const a = machineHarness();
  const b = machineHarness();

  a.machine.event(forOverlay(a.placement({ dialogue: "hi", bubble: true })));
  a.machine.frame(forOverlay(a.placement({ bubble: true })));
  b.machine.event(forOverlay(b.placement({ dialogue: "hi", bubble: false })));
  assert.equal(a.surface(), "speech", "the owner shows the line");
  assert.equal(b.surface(), null, "the other display never latched it");

  // Mid-reading, ownership flips: the shell re-pulses to b and stops naming a.
  a.machine.hideAllNow();
  b.machine.event(forOverlay(b.placement({ dialogue: "hi", bubble: true })));
  b.machine.frame(forOverlay(b.placement({ bubble: true })));
  assert.equal(a.surface(), null, "the old display is already clear");
  assert.equal(b.surface(), "speech", "and the new one shows the same line");
});
