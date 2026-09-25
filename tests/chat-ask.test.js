// What a permission ask says, the one line of copy in the product that stands
// in front of a security decision. The orderings matter as much as the wording:
// a row that leads with `other` and withholds the question teaches a reflexive Allow.

import assert from "node:assert/strict";
import { test } from "node:test";

import { askSays, elicitSays } from "../src/chat-ask.js";

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

function askText(value) {
  const { title, details, metadata } = askSays(value);
  return [title, ...details.map(({ text }) => text), metadata].filter(Boolean).join("\n");
}

test("the content is what the row says, under the title", () => {
  const says = askSays({ ...ask, content: ["Which branch should I push to?"] });

  assert.deepEqual(says, {
    title: "Question from MCP server",
    details: [{ text: "Which branch should I push to?", code: false }],
    metadata: "",
  });
});

test("the arguments stand in when there is no content", () => {
  const says = askSays({
    ...ask,
    title: "Run a command",
    kind: "execute",
    input: { command: "rm -rf /", cwd: "/Users/oded" },
  });

  assert.deepEqual(says, {
    title: "Run a command",
    details: [
      { text: "command: rm -rf /", code: true },
      { text: "cwd: /Users/oded", code: true },
    ],
    metadata: "execute",
  });
});

test("execute content carries a command, while other content is prose", () => {
  assert.deepEqual(
    askSays({ ...ask, title: "Open Calculator", kind: "execute", content: ["open -a Calculator"] }),
    {
      title: "Open Calculator",
      details: [{ text: "open -a Calculator", code: true }],
      metadata: "execute",
    },
  );
  assert.deepEqual(
    askSays({ ...ask, content: ["Which branch?"] }),
    {
      title: "Question from MCP server",
      details: [{ text: "Which branch?", code: false }],
      metadata: "",
    },
  );
});

test("content wins over the arguments rather than joining them", () => {
  const says = askText({
    ...ask,
    content: ["Which branch should I push to?"],
    input: { question: "Which branch should I push to?" },
  });

  assert.ok(!says.includes("question:"), says);
});

test("an ask with nothing to show says so, and says it as a sentence", () => {
  const says = askText({ ...ask, title: null });

  assert.equal(says, "The Harness asked for permission without saying what for.");
  assert.ok(!says.includes("untitled"), says);
  assert.ok(!says.includes("other"), says);
});

test("kind never leads, and `other` never appears at all", () => {
  assert.equal(askText(ask), "Question from MCP server");
  assert.equal(
    askText({ ...ask, kind: "execute" }),
    "Question from MCP server\nexecute",
  );
});

test("a tool that touches a path names the path beside the kind", () => {
  const says = askText({
    ...ask,
    title: "Edit a file",
    kind: "edit",
    locations: ["/Users/oded/src/main.rs"],
  });

  assert.equal(says, "Edit a file\nedit · /Users/oded/src/main.rs");
});

test("more paths than fit are counted, not listed", () => {
  const says = askText({
    ...ask,
    locations: ["/a.rs", "/b.rs", "/c.rs", "/d.rs", "/e.rs"],
  });

  assert.equal(says, "Question from MCP server\n/a.rs, /b.rs, /c.rs, and 2 more paths");
});

test("a huge argument payload is bounded and marked", () => {
  const says = askText({ ...ask, input: { blob: "x".repeat(50_000) } });

  assert.ok(says.length <= 600, `${says.length} characters`);
  assert.ok(says.endsWith("…"), says);
});

// The kind and the paths are drawn after the question, so the budget has to
// cover them as well.
test("the kind and the paths are inside the budget, not after it", () => {
  const says = askText({
    ...ask,
    title: "Edit some files",
    kind: "edit",
    content: ["Q".repeat(1000)],
    locations: ["/a".repeat(100), "/b".repeat(100), "/c".repeat(100)],
  });

  assert.ok(says.length <= 600, `${says.length} characters`);
  assert.ok(says.endsWith("…"), says);
});

// A chatty server, not a hostile one: the row is back to withholding the
// question if a long title can spend the whole budget first.
test("a verbose title cannot crowd out the question", () => {
  const says = askText({
    ...ask,
    title: "Permission ".repeat(60),
    content: ["Which branch should I push to?"],
  });

  assert.ok(says.includes("Which branch should I push to?"), says);
});

test("more arguments than fit are counted, not listed", () => {
  const input = Object.fromEntries(
    Array.from({ length: 20 }, (_, n) => [`arg${n}`, n]),
  );

  const says = askText({ ...ask, input });

  assert.ok(says.includes("and 14 more arguments"), says);
  assert.ok(!says.includes("arg7:"), says);
});

test("a long question is bounded at the same budget the row has", () => {
  const says = askText({ ...ask, content: ["why ".repeat(1000)] });

  assert.ok(says.length <= 600, `${says.length} characters`);
  assert.ok(says.endsWith("…"), says);
});

// An MCP server chooses this text. Newlines and bidi overrides inside it would
// otherwise forge lines the row draws itself — a fake `edit · /safe/path`
// under a question that asks to write somewhere else.
test("untrusted text cannot forge a line of its own", () => {
  const says = askText({
    ...ask,
    title: "Question\nedit · /safe/path",
    content: ["read‮/etc/passwd\ttail"],
  });

  assert.equal(says, "Question edit · /safe/path\nread /etc/passwd tail");
  assert.equal(says.split("\n").length, 2);
});

test("arguments that are not an object still read as one line", () => {
  assert.equal(askText({ ...ask, title: null, input: "rm -rf /" }), '"rm -rf /"');
  assert.equal(askText({ ...ask, title: null, input: ["a", "b"] }), '["a","b"]');
});

test("an ask whose every field is blank is still the sentence", () => {
  const says = askText({
    ...ask,
    title: "   ",
    content: ["", "  "],
    input: {},
  });

  assert.equal(says, "The Harness asked for permission without saying what for.");
});

test("a missing field is not a crash", () => {
  assert.equal(
    askText({ request: "1", options: [] }),
    "The Harness asked for permission without saying what for.",
  );
});

// A kind cannot be the whole of an ask: it is a category, not the question. A
// path can: it is a fact about what happens, not a label for it.
test("an elicitation form says the question the Harness sent", () => {
  assert.equal(
    elicitSays({
      request: "43",
      message: "How should I approach this refactoring?",
      field: "strategy",
      options: [{ value: "balanced", name: "balanced" }],
    }),
    "How should I approach this refactoring?",
  );
});

test("an elicitation with no message says so as a sentence", () => {
  assert.equal(
    elicitSays({ request: "43", message: "  ", field: "", options: [] }),
    "The Harness asked a question without saying what for.",
  );
});

test("untrusted elicitation text cannot forge a line of its own", () => {
  assert.equal(
    elicitSays({
      request: "43",
      message: "Which branch?\nedit · /safe/path",
      field: "branch",
      options: [],
    }),
    "Which branch? edit · /safe/path",
  );
});

test("a kind is not enough on its own, and a path is", () => {
  assert.equal(
    askText({ ...ask, title: null, kind: "execute" }),
    "The Harness asked for permission without saying what for.",
  );
  assert.equal(
    askText({ ...ask, title: null, kind: "edit", locations: ["/a.rs"] }),
    "edit · /a.rs",
  );
});
