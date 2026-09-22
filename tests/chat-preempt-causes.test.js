// The clause table is keyed by the words `director::happened_cell` writes, and
// nothing in either language checks that the two agree. A `Happened` variant
// added on the Rust side would fall to the generic clause and say nothing about
// what the user just did, which is the whole point of the note. #890.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { preemptedNote } from "../src/chat-settle.js";

const director = readFileSync(new URL("../crates/core/src/director.rs", import.meta.url), "utf8");
const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");

// The arms of `happened_cell`, which is the one function that names a wake for
// a surface to draw.
function wakeWords() {
  const at = director.indexOf("pub fn happened_cell(");
  assert.notEqual(at, -1, "happened_cell is missing");
  const body = director.slice(at, director.indexOf("\n}", at));
  return [...body.matchAll(/=>\s*"([^"]+)"/g)].map((match) => match[1]);
}

test("every wake the Rust side can name has a clause of its own", () => {
  const words = wakeWords();
  // Proof the arms parsed, without pinning the list: a new variant should send
  // someone to the clause table, not to this assertion.
  assert.ok(
    words.includes("poked") && words.includes("spoken to"),
    `happened_cell parsed to ${JSON.stringify(words)}, which is not its arms`,
  );

  const generic = preemptedNote("a wake no Rust arm writes");
  for (const word of words) {
    assert.notEqual(
      preemptedNote(word),
      generic,
      `"${word}" falls back to the generic clause instead of naming itself`,
    );
  }
});

test("the transcript draws a preempted turn instead of dropping the row", () => {
  const after = js.split('outcome.action === "preempted")')[1];
  assert.ok(after, "chat.js handles no preempted outcome");

  // Only as far as the next arm, so a `note` call further down cannot pass for
  // this one.
  const branch = after.split("} else")[0];
  assert.match(
    branch,
    /turn\.them\.remove\(\)/,
    "the waiting caret has to go, or the row keeps blinking under the note",
  );
  assert.match(
    branch,
    /note\(outcome\.note\)/,
    "the note must land where the answer would have been",
  );
});
