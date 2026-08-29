// Run with `node --test tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { interpolate } from "../src/interpolate.js";

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
