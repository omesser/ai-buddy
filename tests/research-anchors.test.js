// Run with `node --test tests/`.
//
// #620: a line citation in a research document is read against that document's
// anchor, never against `main`. Without the anchor in the preamble the next
// reader checks the number against their checkout, finds it moved, and files
// rot that is not there. `docs/agents/docs.md` holds the convention.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

const dir = new URL("../docs/research/", import.meta.url);

// `src/model.rs:912`, or a fenced quote's `912:920:src/model.rs` header.
const citesLines = /[\w/.-]+\.rs:\d+|^```\d+:\d+:/m;
// A commit named as one, in backticks: `c3cb15f9`.
const namesAnchor = /`[0-9a-f]{8,40}`/;

for (const name of readdirSync(dir).filter((file) => file.endsWith(".md"))) {
  const text = readFileSync(new URL(name, dir), "utf8");
  if (!citesLines.test(text)) continue;

  test(`${name} cites lines, so its preamble names an anchor`, () => {
    // The preamble is everything above the first horizontal rule.
    const [preamble] = text.split("\n---");
    assert.match(
      preamble,
      namesAnchor,
      "a document citing line numbers must name the commit they are against",
    );
  });
}
