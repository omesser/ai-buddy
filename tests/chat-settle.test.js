// An empty `said` after Speech has already been drawn is not a missing answer;
// treating it as one is the production change that fails this file.

import assert from "node:assert/strict";
import { test } from "node:test";

import { MISSING_ANSWER, createChatTurns } from "../src/chat-settle.js";

function play(turns, payloads) {
  const log = [];
  for (const payload of payloads) {
    const outcome = turns.settle(payload);
    if (outcome.action === "speech") {
      log.push(outcome.said);
    } else if (outcome.action === "missing") {
      log.push(outcome.note);
    } else if (outcome.action === "error") {
      log.push(outcome.note);
    }
  }
  return log;
}

test("Speech already drawn, then said None, does not report a missing answer", () => {
  const turns = createChatTurns();
  turns.typed();
  turns.typed();

  const log = play(turns, [
    {
      said: "You may! Tell me one thing that would make me a more useful little buddy.",
    },
    { said: null },
  ]);

  assert.deepEqual(log, [
    "You may! Tell me one thing that would make me a more useful little buddy.",
  ]);
  assert.equal(
    log.includes(MISSING_ANSWER),
    false,
    "an empty settle after Speech landed is not a missing answer",
  );
});

test("a turn that produced no Speech still reports a missing answer", () => {
  const turns = createChatTurns();
  turns.typed();

  const log = play(turns, [{ said: null }]);

  assert.deepEqual(log, [MISSING_ANSWER]);
});

test("a later turn that produced no Speech still reports a missing answer", () => {
  const turns = createChatTurns();
  turns.typed();
  play(turns, [{ said: "You may!" }]);
  turns.typed();

  const log = play(turns, [{ said: null }]);

  assert.deepEqual(log, [MISSING_ANSWER]);
});

test("a superseded caret is not reported as a missing answer", () => {
  const turns = createChatTurns();
  turns.typed();

  const log = play(turns, [{ said: null, superseded: true }]);

  assert.deepEqual(log, []);
});

test("a Harness error is still the error, not a missing answer", () => {
  const turns = createChatTurns();
  turns.typed();

  const log = play(turns, [{ said: null, error: "model not found" }]);

  assert.deepEqual(log, ["The Harness reported an error: model not found"]);
});
