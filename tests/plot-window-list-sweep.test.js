import assert from "node:assert/strict";
import { test } from "node:test";
import { parseSweep, renderSvg } from "../scripts/plot-window-list-sweep.mjs";

const tsv = [
  "scenario\twindows\tpolls_hz\tmedian_us\tp95_us\tmax_us\tnotes",
  "sweep-micro+100\t153\tN/A\t1200.5\t1500.0\t2000.0\tin-process",
  "sweep-micro+0\t53\tN/A\t404.0\t447.0\t1856.0\tin-process",
  "idle+0\t55\t9.80\t2169\t5076\t8057\tdtrace 147 calls",
  "idle+100\tN/A\tN/A\tN/A\tN/A\tN/A\twindow flood never reported",
  "riding+0\t56\t45.40\t1500\t4750\t15165\tgliding perch",
].join("\n");

test("sweep rows become per-series points sorted by window count, skipping failed rows", () => {
  const points = Object.fromEntries(parseSweep(tsv).map((s) => [s.label, s.points]));
  assert.deepEqual(points, {
    "fidget idle poll, p95": [{ x: 55, y: 5076 }],
    "fidget idle poll, median": [{ x: 55, y: 2169 }],
    "in-process microbench, median": [
      { x: 53, y: 404 },
      { x: 153, y: 1200.5 },
    ],
  });
});

test("the plot draws one polyline per series and the frame budget line", () => {
  const svg = renderSvg(parseSweep(tsv));
  assert.equal(svg.match(/<polyline /g).length, 3);
  assert.match(svg, /one 60 Hz frame \(16\.7 ms\)/);
});
