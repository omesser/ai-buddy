// Run with `node --test tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { arrived, interpolate, onDisplay } from "../src/interpolate.js";

const at = (x, y, ms) => ({ x, y, at: ms });

test("it draws one sample behind, arriving as the next placement lands", () => {
  const previous = at(100, 200, 1000);
  const latest = at(140, 200, 1020);

  assert.deepEqual(interpolate(previous, latest, 1020), { x: 100, y: 200 });
  assert.deepEqual(interpolate(previous, latest, 1030), { x: 120, y: 200 });
  assert.deepEqual(interpolate(previous, latest, 1040), { x: 140, y: 200 });
});

test("both axes move together", () => {
  // Halfway through the 20ms that follow the latest placement, which is the
  // window it is drawn across.
  const drawn = interpolate(at(0, 0, 0), at(80, 40, 20), 30);
  assert.deepEqual(drawn, { x: 40, y: 20 });
});

test("a late display frame stops at the latest placement rather than overshooting", () => {
  const previous = at(0, 0, 0);
  const latest = at(100, 0, 20);

  // 100ms after a placement meant to cover 20ms. Extrapolating would put the
  // sprite at x=500, somewhere the Engine never said it was.
  assert.deepEqual(interpolate(previous, latest, 120), { x: 100, y: 0 });
});

test("an early display frame does not reverse back past the previous placement", () => {
  assert.deepEqual(interpolate(at(0, 0, 100), at(50, 0, 120), 90), { x: 0, y: 0 });
});

test("two placements in the same millisecond draw the latest", () => {
  assert.deepEqual(interpolate(at(0, 0, 50), at(90, 10, 50), 50), { x: 90, y: 10 });
});

test("a clock that goes backwards draws the latest rather than dividing by a negative span", () => {
  assert.deepEqual(interpolate(at(0, 0, 80), at(90, 10, 20), 25), { x: 90, y: 10 });
});

// `arrived` decides when the renderer stops asking for display frames. A false
// it never returns is a sprite that stutters; a true it returns too early is a
// loop that stops and never comes back.

test("a sprite still crossing the gap has not arrived", () => {
  assert.equal(arrived(at(100, 200, 1000), at(140, 200, 1020), 1030), false);
});

test("a sprite has arrived once a sample interval has passed", () => {
  assert.equal(arrived(at(100, 200, 1000), at(140, 200, 1020), 1040), true);
});

test("two placements in the same place are arrived however far apart they fell", () => {
  assert.equal(arrived(at(100, 200, 1000), at(100, 200, 1250), 1001), true);
});

test("the first placement of all has nowhere to have come from", () => {
  assert.equal(arrived(null, at(100, 200, 1000), 1000), true);
});

// `onDisplay` decides whether this overlay asks its display for a frame at all
// (#764). A true it returns too often keeps a quiet display's CVDisplayLink
// warm; a false it returns on a sprite that is really there stops the drawing.

const display = { width: 1920, height: 1080 };
const rect = (x, y) => ({ x, y, width: 96, height: 96 });

test("a sprite in the middle of this display is on it", () => {
  assert.equal(onDisplay(null, rect(900, 500), display, 8), true);
});

test("a sprite living entirely on the panel to the left is not on this one", () => {
  assert.equal(onDisplay(null, rect(-500, 500), display, 8), false);
});

test("a sprite living entirely on the panel to the right is not on this one", () => {
  assert.equal(onDisplay(null, rect(2400, 500), display, 8), false);
});

test("a sprite above or below this display is not on it", () => {
  assert.equal(onDisplay(null, rect(900, -300), display, 8), false);
  assert.equal(onDisplay(null, rect(900, 1400), display, 8), false);
});

test("a sprite straddling the seam is on both displays at once", () => {
  // The same Character, in each overlay's own coordinates: 40px of it past the
  // right edge of a 1920-wide display is 40px inside the one beyond it.
  assert.equal(onDisplay(null, rect(1880, 500), display, 8), true);
  assert.equal(onDisplay(null, rect(-40, 500), display, 8), true);
});

test("a sprite one pixel past the edge is on the neighbour and not on this one", () => {
  assert.equal(onDisplay(null, rect(1920, 500), display, 0), false);
  assert.equal(onDisplay(null, rect(0, 500), display, 0), true);
});

test("a sprite just off the edge is still on this display while inside the margin", () => {
  assert.equal(onDisplay(null, rect(1924, 500), display, 8), true);
  assert.equal(onDisplay(null, rect(-102, 500), display, 8), true);
});

test("a sprite past the margin is off this display", () => {
  assert.equal(onDisplay(null, rect(1930, 500), display, 8), false);
  assert.equal(onDisplay(null, rect(-110, 500), display, 8), false);
});

test("a sprite leaving this display keeps it drawing until the exit is drawn", () => {
  assert.equal(onDisplay(rect(1800, 500), rect(2400, 500), display, 8), true);
  assert.equal(onDisplay(rect(2400, 500), rect(3000, 500), display, 8), false);
});
