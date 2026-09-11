// Run with `node --test tests/`.
//
// #610: the strip above the composer draws two things now — the Harness's
// thinking (ADR-0025) and the mark for a reply the cap ended. They share one
// line, and the orderings below are the ones a window will actually deliver.

import assert from "node:assert/strict";
import { test } from "node:test";

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

// The whole of the ADR-0025 caution: the clear fires at the end of the turn,
// which is the moment this mark appears. Either arrival order has to leave the
// mark up.
test("the end-of-turn clear does not sweep the mark away", () => {
  const late = stripped();
  late.strip.thinking("weighing a nap");
  late.strip.answered(true);
  late.strip.thinking("");
  assert.equal(
    late.showing(),
    TRUNCATED_MARK,
    "a clear arriving after the mark leaves it standing",
  );

  const early = stripped();
  early.strip.thinking("weighing a nap");
  early.strip.thinking("");
  early.strip.answered(true);
  assert.equal(
    early.showing(),
    TRUNCATED_MARK,
    "a clear arriving before the mark does not take its place",
  );
});

test("a whole reply leaves nothing in the strip", () => {
  const { strip, showing } = stripped();

  strip.answered(true);
  strip.answered(false);
  assert.equal(showing(), "");
});

test("the mark stays up until the next turn starts", () => {
  const { strip, showing } = stripped();

  strip.answered(true);
  assert.equal(showing(), TRUNCATED_MARK);

  strip.thinking("");
  assert.equal(showing(), TRUNCATED_MARK, "nothing came in, so it stays");

  strip.thinking("weighing the next one");
  assert.equal(
    showing(),
    "weighing the next one",
    "a thought means a turn is running, and it outranks the last one's mark",
  );

  strip.thinking("");
  assert.equal(showing(), "", "the mark went with the turn that replaced it");
});

test("the user's next line clears the mark", () => {
  const { strip, showing } = stripped();

  strip.answered(true);
  strip.asked();
  assert.equal(showing(), "");
});

// The bubble draws the same glyph, so there is one string and not two that
// drift apart.
test("the mark is the one the bubble draws", () => {
  assert.equal(TRUNCATED_MARK, "[response truncated]");
});
