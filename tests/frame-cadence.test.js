import assert from "node:assert/strict";
import { test } from "node:test";
import { analyze, report } from "../scripts/frame-cadence.mjs";

// Five Engine ticks, one 40 ms late, and the display frames an overlay drew
// across them: a quiet loop restarts on the late arrival, then misses a vsync
// and holds the sprite at the latest placement.
const log = [
  "cadence: overlay-0 900.000 1 880.000 890.000",
  "frame: 1000 Grounded pos(1,2) sprite(3,4) walk#0 BMO",
  "overlay: overlay-0 covers 1512x982 at (0,0)",
  "frame: 1016 Grounded pos(1,2) sprite(3,4) walk#1 BMO",
  "cadence: overlay-0 1017.000 1 1000.000 1016.500",
  "cadence: overlay-1 1025.000 1 - 1016.500",
  "frame: 1033 Grounded pos(1,2) sprite(3,4) walk#2 BMO",
  "cadence: overlay-0 1033.667 1 1016.500 1033.200",
  "frame: 1050 Grounded pos(1,2) sprite(3,4) walk#3 BMO",
  "cadence: overlay-0 1050.333 0 1033.200 1050.000",
  "frame: 1090 Grounded pos(1,2) sprite(3,4) walk#4 BMO",
  "cadence: overlay-0 1100.333 1 1050.000 1090.500",
  "cadence: overlay-0 1117.000 1 1050.000 1090.500",
  "cadence: overlay-0 1150.333 0 1050.000 1090.500",
  "frame: 2000 Grounded pos(1,2) sprite(3,4) walk#5 BMO",
].join("\n");

test("a window of the log reduces to cadence, drops, and interpolation lag", () => {
  assert.deepEqual(analyze(log, { from: 1000, to: 1200 }), {
    frames: 7,
    fps: 48,
    drops: 1,
    restarts: 1,
    ticks: 5,
    tickHz: 44.4,
    lag: { p50Ms: 16.8, p95Ms: 59.8, p50Samples: 1, p95Samples: 1.48, held: 1 },
    histogram: [
      { bin: "0-10", frames: 0, ticks: 0 },
      { bin: "10-14", frames: 0, ticks: 0 },
      { bin: "14-18", frames: 3, ticks: 3 },
      { bin: "18-20", frames: 0, ticks: 0 },
      { bin: "20-25", frames: 0, ticks: 0 },
      { bin: "25-34", frames: 1, ticks: 0 },
      { bin: "34-50", frames: 0, ticks: 1 },
      { bin: "50+", frames: 0, ticks: 0 },
    ],
  });
});

test("a window with no display frames says so rather than dividing by zero", () => {
  const idle = analyze("frame: 1000 Perched pos(1,2) sprite(3,4) idle#0 BMO", {});
  assert.equal(idle.frames, 0);
  assert.equal(idle.fps, null);
  assert.equal(idle.lag, null);
  assert.match(report(idle), /\| Display frames \| 0 \|/);
});

test("the report is a Markdown table a baseline doc can paste", () => {
  const text = report(analyze(log, { from: 1000, to: 1200 }));
  assert.match(text, /\| Mean fps \| 48 \|/);
  assert.match(text, /\| Dropped \(>20 ms\) \| 1 \|/);
  assert.match(text, /\| Interpolation lag p50 \| 16\.8 ms \(1 samples\) \|/);
  assert.match(text, /\| 25-34 \| 1 \| 0 \|/);
});
