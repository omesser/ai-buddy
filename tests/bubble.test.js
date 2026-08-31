// Run with `node --test tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { bubbleDuration, wrapText, placeBubble, CEILING_CLEARANCE } from "../src/bubble.js";

const testMeasureFn = (text) => ({ width: text.length * 8 });

test("bubble duration is 900ms + 55ms per character, clamped to 2-8s", () => {
  assert.equal(bubbleDuration("hi"), 2000, "min clamp");
  assert.equal(bubbleDuration("hello"), 2000, "still at min");
  // "hello there" = 11 chars = 900 + 605 = 1505ms, clamped to 2000ms min
  assert.equal(bubbleDuration("hello there"), 2000, "short text clamped to min");
  // Longer text that exceeds min
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
