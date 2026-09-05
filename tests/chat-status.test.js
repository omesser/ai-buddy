// Run with `node --test tests/`.
//
// The status bar's arithmetic: what each cell says for one push, and the
// countdown the window runs between pushes. What the bar looks like is not
// tested here and cannot be — it is a stylesheet.

import assert from "node:assert/strict";
import { test } from "node:test";

import { statusCells, untilWake } from "../src/chat-status.js";

// One push, as the Shell serializes it.
const push = {
  behavior: "prowl",
  primitive: "Walk",
  animation: "walk",
  state: "Grounded",
  happened: "poked",
  facing: 1,
  asking: false,
};

test("a push fills every cell", () => {
  const cells = statusCells(push, 12_000);

  assert.equal(cells.behavior, "prowl");
  assert.equal(cells.primitive, "Walk");
  assert.equal(cells.animation, "walk");
  assert.equal(cells.state, "Grounded");
  assert.equal(cells.happened, "poked");
  assert.equal(cells.facing, "→");
  assert.equal(cells.director, "wake 12s");
});

test("the Engine's own moments draw a dash, not a blank", () => {
  const cells = statusCells({ ...push, behavior: null, primitive: null }, 1000);

  assert.equal(cells.behavior, "—");
  assert.equal(cells.primitive, "—");
});

test("facing left mirrors the arrow", () => {
  assert.equal(statusCells({ ...push, facing: -1 }, 1000).facing, "←");
});

test("a turn on the wire is said as well as the countdown", () => {
  assert.equal(statusCells({ ...push, asking: true }, 5000).director, "thinking · wake 5s");
});

test("no wake coming reads as a dash rather than a number", () => {
  assert.equal(statusCells(push, null).director, "wake —");
});

test("before the first push every cell says so", () => {
  const cells = statusCells(null, null);

  assert.equal(cells.behavior, "—");
  assert.equal(cells.state, "—");
  assert.equal(cells.director, "wake —");
  // No arrow: a sprite nothing has reported on faces no way.
  assert.equal(cells.facing, "");
});

test("the countdown rounds up, and stops at due", () => {
  assert.equal(untilWake(11_400), "12s");
  assert.equal(untilWake(1), "1s");
  assert.equal(untilWake(0), "due");
  // The window keeps counting between pushes, and a wake the Engine has not
  // reached yet is due rather than negative.
  assert.equal(untilWake(-3000), "due");
});
