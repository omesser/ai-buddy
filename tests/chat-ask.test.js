// Run with `node --test tests/`.
//
// What a permission ask says, which is the one line of copy in the product
// that stands in front of a security decision (#678). The orderings matter as
// much as the wording: a row that leads with `other` and withholds the
// question is the shape that teaches a reflexive Allow.

import assert from "node:assert/strict";
import { test } from "node:test";

import { askSays } from "../src/chat-ask.js";

// One ask, as the Shell serializes it.
const ask = {
  request: "99",
  title: "Question from MCP server",
  kind: "other",
  content: [],
  input: null,
  locations: [],
  options: [],
};

test("the content is what the row says, under the title", () => {
  const says = askSays({ ...ask, content: ["Which branch should I push to?"] });

  assert.equal(says, "Question from MCP server\nWhich branch should I push to?");
});

test("the arguments stand in when there is no content", () => {
  const says = askSays({
    ...ask,
    title: "Run a command",
    kind: "execute",
    input: { command: "rm -rf /", cwd: "/Users/oded" },
  });

  assert.equal(says, "Run a command\ncommand: rm -rf /\ncwd: /Users/oded\nexecute");
});

test("content wins over the arguments rather than joining them", () => {
  const says = askSays({
    ...ask,
    content: ["Which branch should I push to?"],
    input: { question: "Which branch should I push to?" },
  });

  assert.ok(!says.includes("question:"), says);
});

test("an ask with nothing to show says so, and says it as a sentence", () => {
  const says = askSays({ ...ask, title: null });

  assert.equal(says, "The Harness asked for permission without saying what for.");
  assert.ok(!says.includes("untitled"), says);
  assert.ok(!says.includes("other"), says);
});

test("kind never leads, and `other` never appears at all", () => {
  assert.equal(askSays(ask), "Question from MCP server");
  assert.equal(
    askSays({ ...ask, kind: "execute" }),
    "Question from MCP server\nexecute",
  );
});

test("a tool that touches a path names the path beside the kind", () => {
  const says = askSays({
    ...ask,
    title: "Edit a file",
    kind: "edit",
    locations: ["/Users/oded/src/main.rs"],
  });

  assert.equal(says, "Edit a file\nedit · /Users/oded/src/main.rs");
});

test("more paths than fit are counted, not listed", () => {
  const says = askSays({
    ...ask,
    locations: ["/a.rs", "/b.rs", "/c.rs", "/d.rs", "/e.rs"],
  });

  assert.equal(says, "Question from MCP server\n/a.rs, /b.rs, /c.rs, and 2 more paths");
});

test("a huge argument payload is bounded and marked", () => {
  const says = askSays({ ...ask, input: { blob: "x".repeat(50_000) } });

  assert.ok(says.length < 800, `${says.length} characters`);
  assert.ok(says.endsWith("…"), says);
});

test("more arguments than fit are counted, not listed", () => {
  const input = Object.fromEntries(
    Array.from({ length: 20 }, (_, n) => [`arg${n}`, n]),
  );

  const says = askSays({ ...ask, input });

  assert.ok(says.includes("and 14 more arguments"), says);
  assert.ok(!says.includes("arg7:"), says);
});

test("a long question is bounded at the same budget the row has", () => {
  const says = askSays({ ...ask, content: ["why ".repeat(1000)] });

  assert.ok(says.length < 800, `${says.length} characters`);
  assert.ok(says.endsWith("…"), says);
});

// An MCP server chooses this text. Newlines and bidi overrides inside it would
// otherwise forge lines the row draws itself — a fake `edit · /safe/path`
// under a question that asks to write somewhere else.
test("untrusted text cannot forge a line of its own", () => {
  const says = askSays({
    ...ask,
    title: "Question\nedit · /safe/path",
    content: ["read‮/etc/passwd\ttail"],
  });

  assert.equal(says, "Question edit · /safe/path\nread /etc/passwd tail");
  assert.equal(says.split("\n").length, 2);
});

test("arguments that are not an object still read as one line", () => {
  assert.equal(askSays({ ...ask, title: null, input: "rm -rf /" }), '"rm -rf /"');
  assert.equal(askSays({ ...ask, title: null, input: ["a", "b"] }), '["a","b"]');
});

test("an ask whose every field is blank is still the sentence", () => {
  const says = askSays({
    ...ask,
    title: "   ",
    content: ["", "  "],
    input: {},
  });

  assert.equal(says, "The Harness asked for permission without saying what for.");
});

test("a missing field is not a crash", () => {
  assert.equal(
    askSays({ request: "1", options: [] }),
    "The Harness asked for permission without saying what for.",
  );
});

// The bug was a row that offered a category in place of the question, so a
// kind cannot be the whole of an ask. A path can: it is a fact about what
// happens, not a label for it.
test("a kind is not enough on its own, and a path is", () => {
  assert.equal(
    askSays({ ...ask, title: null, kind: "execute" }),
    "The Harness asked for permission without saying what for.",
  );
  assert.equal(
    askSays({ ...ask, title: null, kind: "edit", locations: ["/a.rs"] }),
    "edit · /a.rs",
  );
});
