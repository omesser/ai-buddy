import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

const html = readFileSync(new URL("../docs/design/cues.html", import.meta.url), "utf8");

function loadFlight() {
  const start = html.indexOf("const GRAVITY = 3600;");
  const endMark = "const flight = { apply, SILL };";
  const end = html.indexOf(endMark, start);
  assert.ok(start !== -1 && end > start, "flight slice");
  const slice = html.slice(start, end + endMark.length);
  const sandbox = {};
  vm.createContext(sandbox);
  vm.runInContext(`${slice}\nglobalThis.SILL = SILL;\n`, sandbox);
  return sandbox;
}

function world(flight) {
  return {
    width: 800,
    sprite: { w: 126, h: 128 },
    ledges: [
      flight.SILL,
      { id: "floor", y: 300, x0: -Infinity, x1: Infinity },
    ],
  };
}

const dt = 1 / 32;

test("an upward throw rises on the next tick", () => {
  const flight = loadFlight();
  const out = flight.apply(
    { mode: "falling", x: 400, y: 200, vx: 200, vy: -400, facing: 1 },
    world(flight),
    { type: "tick", dt, walk: false },
  );
  assert.equal(out.pose.mode, "falling");
  assert.equal(out.pose.x, 406.25);
  assert.equal(out.pose.y, 191.015625);
  assert.equal(out.pose.vy, -287.5);
  assert.equal(out.pose.facing, 1);
});

test("a rising throw stops at the top of the sprite instead of leaving the desk", () => {
  const flight = loadFlight();
  const out = flight.apply(
    { mode: "falling", x: 400, y: 130, vx: 200, vy: -400, facing: 1 },
    world(flight),
    { type: "tick", dt, walk: false },
  );
  assert.equal(out.pose.mode, "falling");
  assert.equal(out.pose.x, 406.25);
  assert.equal(out.pose.y, 128);
  assert.equal(out.pose.vy, 0);
});

test("a leftward throw faces left", () => {
  const flight = loadFlight();
  const out = flight.apply(
    grabbing(180, 140),
    world(flight),
    { type: "release", sample: { dx: -6, dy: 0, elapsedMs: 30 } },
  );
  assert.equal(out.cue, "throw");
  assert.equal(out.pose.vx, -200);
  assert.equal(out.pose.facing, -1);
  assert.equal(out.pose.y, 140);
});

test("a still fall from the top of the desk is airborne one tick later", () => {
  const flight = loadFlight();
  const out = flight.apply(
    { mode: "falling", x: 400, y: 0, vx: 0, vy: 0, facing: 1 },
    world(flight),
    { type: "tick", dt, walk: false },
  );
  assert.equal(out.pose.y, 3.515625);
  assert.equal(out.pose.vy, 112.5);
  assert.equal(out.pose.mode, "falling");
});

test("a fall onto the sill rests there", () => {
  const flight = loadFlight();
  const out = flight.apply(
    { mode: "falling", x: 180, y: 161, vx: 0, vy: 0, facing: 1 },
    world(flight),
    { type: "tick", dt, walk: false },
  );
  assert.equal(out.pose.mode, "resting");
  assert.equal(out.pose.on, "sill");
  assert.equal(out.feet.y, 164);
  assert.equal(out.cue, null);
});

test("a walk off the right end of the sill falls", () => {
  const flight = loadFlight();
  const out = flight.apply(
    { mode: "resting", x: 233, on: "sill", facing: 1 },
    world(flight),
    { type: "tick", dt, walk: true },
  );
  assert.equal(out.pose.mode, "falling");
  assert.equal(out.pose.x, 236.75);
  assert.equal(out.pose.y, 164);
  assert.equal(out.pose.vx, 120);
});

test("a walk off the left end of the sill falls", () => {
  const flight = loadFlight();
  const out = flight.apply(
    { mode: "resting", x: 99, on: "sill", facing: -1 },
    world(flight),
    { type: "tick", dt, walk: true },
  );
  assert.equal(out.pose.mode, "falling");
  assert.equal(out.pose.x, 95.25);
  assert.equal(out.pose.y, 164);
  assert.equal(out.pose.vx, -120);
});

test("a body under the sill misses it, and one just above the floor rests there", () => {
  const flight = loadFlight();
  const desk = world(flight);
  const tick = { type: "tick", dt, walk: false };
  const missed = flight.apply(
    { mode: "falling", x: 180, y: 200, vx: 0, vy: 0, facing: 1 },
    desk,
    tick,
  );
  assert.equal(missed.pose.mode, "falling");
  assert.equal(missed.pose.on, undefined);
  const landed = flight.apply(
    { mode: "falling", x: 180, y: 297, vx: 0, vy: 0, facing: 1 },
    desk,
    tick,
  );
  assert.equal(landed.pose.mode, "resting");
  assert.equal(landed.pose.on, "floor");
  assert.equal(landed.feet.y, 300);
});

function grabbing(x, y) {
  return {
    mode: "grabbing",
    x,
    y,
    facing: 1,
    cursor: { x: 0, y: 0 },
    travel: { x: 0, y: 0 },
    travelMs: 0,
    prior: { x: 0, y: 0 },
    priorMs: 0,
  };
}

test("a fast release throws and keeps the grab feet", () => {
  const flight = loadFlight();
  const desk = world(flight);
  const sampled = flight.apply(
    grabbing(180, 140),
    desk,
    { type: "sample", dx: 6, dy: -3, elapsedMs: 30 },
  );
  const out = flight.apply(sampled.pose, desk, { type: "release" });
  assert.equal(out.pose.x, 180);
  assert.equal(out.pose.y, 140);
  assert.equal(out.pose.vx, 200);
  assert.equal(out.pose.vy, -100);
  assert.equal(out.cue, "throw");
});

test("a slow release drops in place with no velocity", () => {
  const flight = loadFlight();
  const out = flight.apply(
    grabbing(180, 140),
    world(flight),
    { type: "release", sample: { dx: 1, dy: 0, elapsedMs: 30 } },
  );
  assert.equal(out.cue, "drop");
  assert.equal(out.pose.vx, 0);
  assert.equal(out.pose.vy, 0);
  assert.equal(out.pose.x, 180);
  assert.equal(out.pose.y, 140);
});

test("three samples of stillness after a shove release as a drop", () => {
  const flight = loadFlight();
  const desk = world(flight);
  let pose = grabbing(180, 140);
  pose = flight.apply(pose, desk, { type: "sample", dx: 9, dy: 0, elapsedMs: 30 }).pose;
  pose = flight.apply(pose, desk, { type: "sample", dx: 0, dy: 0, elapsedMs: 30 }).pose;
  const out = flight.apply(pose, desk, { type: "release", sample: { dx: 0, dy: 0, elapsedMs: 30 } });
  assert.equal(out.cue, "drop");
  assert.equal(out.pose.vx, 0);
  assert.equal(out.pose.vy, 0);
});
