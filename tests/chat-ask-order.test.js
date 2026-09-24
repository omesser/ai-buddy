// A typed turn opens its caret row when the line is sent. An ask the Harness
// raises mid-turn is appended below it, so the answer that comes after the
// user's Yes fills the row above the ask and reads as if BMO had answered
// first and then asked and gone quiet. The caret has to move under the ask.
// `scripts/chat-ask-order.mjs` drives the real surface headless.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { createChatTurns } from "../src/chat-settle.js";

const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");

function fnBody(name) {
  const at = js.indexOf(`function ${name}(`);
  assert.notEqual(at, -1, `${name} is missing`);
  const from = js.indexOf("{", at);
  let depth = 0;
  for (let i = from; i < js.length; i += 1) {
    if (js[i] === "{") {
      depth += 1;
    } else if (js[i] === "}") {
      depth -= 1;
      if (depth === 0) {
        return js.slice(from, i + 1);
      }
    }
  }
  assert.fail(`${name} has no matching close`);
}

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

test("an ask and a form both move the waiting caret below themselves", () => {
  for (const name of ["asked", "elicited"]) {
    assert.match(fnBody(name), /lowerCaret\(\)/, `${name} leaves the caret above the ask`);
  }
  assert.match(fnBody("lowerCaret"), /turns\.newest\(\)/);
});
