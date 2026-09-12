// Run with `node --test tests/`.
//
// #610: the strip above the composer is for the Harness's thinking (ADR-0025).
// The truncation mark rides in the remembered text (session + Chat history),
// not here.

import assert from "node:assert/strict";
import { test } from "node:test";

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
