// A renderer test that needs more than 90 seconds is slow for a reason. Find it
// (a launch per case, a missing kill, a cold start) and measure before raising
// a budget; #1223 cut 120s, 400s and 780s budgets that had hidden one.
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

const CEILING_MS = 90000;

function overBudget(source) {
  return [...source.matchAll(/\btimeout:\s*([\d_]+)/g)]
    .map(([, ms]) => ms)
    .filter((ms) => Number(ms.replaceAll("_", "")) > CEILING_MS);
}

test("no test budget runs past 90 seconds", () => {
  assert.deepEqual(overBudget(`{ skip, timeout: ${4e5} }`), ["400000"]);
  const dir = new URL("./", import.meta.url);
  const over = readdirSync(dir)
    .filter((file) => file.endsWith(".test.js"))
    .flatMap((file) => overBudget(readFileSync(new URL(file, dir), "utf8")).map((ms) => `${file}: ${ms}`));
  assert.deepEqual(over, []);
});
