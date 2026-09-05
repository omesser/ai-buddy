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

test("a turn on the wire displaces the countdown it would reset anyway", () => {
  assert.equal(statusCells({ ...push, asking: true }, 5000).director, "thinking");
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

test("the countdown changes unit rather than growing", () => {
  assert.equal(untilWake(59_000), "59s");
  assert.equal(untilWake(60_000), "1m");
  assert.equal(untilWake(119_000), "2m");
  // `Pace::CAP`, the longest wait the ambient arm ever counts down to.
  assert.equal(untilWake(7_200_000), "2h");
});

// The bar draws on one line and never wraps, so the cells whose vocabulary is
// closed have to fit whatever the two a Character Package names hold. Measured
// here because the stylesheet cannot be run: `.bar .elide` lets those two — and
// only those two — shrink, and everything measured below is a cell that will
// not, so the moment this sum passes the window the cell at the end of the line
// is clipped with no ellipsis to say it was. That is the bug this measurement
// exists to catch a second time.
//
// A monospace advance is 0.6em; the bar is 10.5px, in a window with a 16-point
// gutter each side and 4 points between its twelve cells. 320 is the narrowest
// the surface may be dragged to (`min_inner_size` in `main.rs`), so it is the
// width that has to hold, not the 420 it opens at.
const ADVANCE = 0.6 * 10.5;
const GUTTERS = 2 * 16 + 11 * 4;
const NARROWEST = 320;

test("the closed vocabularies fit the narrowest the window goes", () => {
  const widest = statusCells(
    {
      ...push,
      // Every fixed cell at its longest: `Sleep`, `React` and `Chase` are the
      // longest Primitives, `Grounded` and `Climbing` the longest States, and
      // `spoken to` the longest word `director::happened_cell` writes.
      primitive: "Sleep",
      state: "Grounded",
      happened: "spoken to",
      facing: -1,
    },
    59_000,
  );

  // Everything but the two the `elide` class lets shrink, plus the five
  // separator glyphs the markup puts between them.
  const fixed =
    widest.primitive.length +
    widest.state.length +
    widest.facing.length +
    widest.director.length +
    widest.happened.length +
    5;

  assert.ok(
    fixed * ADVANCE + GUTTERS <= NARROWEST,
    `the fixed cells want ${Math.ceil(fixed * ADVANCE + GUTTERS)}pt of ${NARROWEST}`,
  );
});
