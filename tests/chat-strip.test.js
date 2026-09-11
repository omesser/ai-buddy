// Run with `node --test tests/`.
//
// #610: the strip above the composer is for the Harness's thinking (ADR-0025).
// The truncation mark rides in the remembered text (session + Chat history),
// not here.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { TRUNCATED_MARK } from "../src/bubble.js";
import { createStrip } from "../src/chat-strip.js";

function stripped() {
  let line = "";
  const strip = createStrip((latest) => {
    line = latest;
  });
  return { strip, showing: () => line };
}

test("a thought is drawn and the end of the turn takes it away", () => {
  const { strip, showing } = stripped();

  strip.thinking("weighing a nap against the desk");
  assert.equal(showing(), "weighing a nap against the desk");

  strip.thinking("");
  assert.equal(showing(), "", "an empty line is the turn saying it stopped");
});

test("the user's next line clears the thought", () => {
  const { strip, showing } = stripped();

  strip.thinking("weighing a nap");
  strip.asked();
  assert.equal(showing(), "");
});

// Three places write this string — the bubble, the Chat strip (no longer),
// and the Rust side that puts it in the session the next turn is built from.
// A copy that drifts is a session marked with words no surface draws.
test("the mark is the one the bubble and the Shell use", () => {
  assert.equal(TRUNCATED_MARK, "[response truncated]");

  const dir = dirname(fileURLToPath(import.meta.url));
  const core = readFileSync(join(dir, "../crates/core/src/director.rs"), "utf8");
  const declared = core.match(/pub const TRUNCATED_MARK: &str = "([^"]*)";/);
  assert.ok(declared, "the Shell declares the mark once");
  assert.equal(declared[1], TRUNCATED_MARK);
});
