// The strip above the composer is for the Harness's thinking (ADR-0025). The
// truncation mark rides in the remembered text, not here.

import assert from "node:assert/strict";
import { test } from "node:test";

import { createStrip } from "../src/chat-strip.js";

function stripped() {
  let showing = { line: "", collapsed: false };
  const strip = createStrip((latest) => {
    showing = latest;
  });
  return { strip, showing: () => showing };
}

test("a thought is drawn and the end of the turn takes it away", () => {
  const { strip, showing } = stripped();

  strip.thinking("weighing a nap against the desk");
  assert.deepEqual(showing(), {
    line: "weighing a nap against the desk",
    collapsed: false,
  });

  strip.thinking("");
  assert.deepEqual(
    showing(),
    { line: "", collapsed: false },
    "an empty line is the turn saying it stopped",
  );
});

test("the user's next line clears the thought", () => {
  const { strip, showing } = stripped();

  strip.thinking("weighing a nap");
  strip.asked();
  assert.deepEqual(showing(), { line: "", collapsed: false });
});

test("the user's collapsed preference survives the next turn", () => {
  const { strip, showing } = stripped();

  strip.thinking("weighing a nap");
  strip.toggle();
  strip.thinking("");
  strip.thinking("checking the desk first");

  assert.deepEqual(showing(), {
    line: "checking the desk first",
    collapsed: true,
  });
});
