// A typed turn opens its caret row when the line is sent. An ask the Harness
// raises mid-turn is appended below it, so the answer that comes after the
// user's Yes fills the row above the ask and reads as if BMO had answered
// first and then asked and gone quiet. The caret has to move under the ask.
// `scripts/chat-ask-order.mjs` drives the real surface headless.

import assert from "node:assert/strict";
import { test } from "node:test";

import { createChatTurns } from "../src/chat-settle.js";

test("the newest waiting turn is the one an ask lands under", () => {
  const turns = createChatTurns();
  assert.equal(turns.newest(), null);
  const first = turns.typed();
  const second = turns.typed();
  assert.equal(turns.newest(), second);
  turns.drop(second);
  assert.equal(turns.newest(), first);
  turns.settle({ said: "hi" });
  assert.equal(turns.newest(), null);
});

// DROPPED (#1268): "an ask and a form both move the waiting caret below themselves"
// matches lowerCaret() in the ask function source. It does not draw a row.
