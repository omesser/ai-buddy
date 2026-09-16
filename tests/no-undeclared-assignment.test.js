// Run with `node --test tests/`.
//
// The webview loads as `<script type="module">`, so every file in `src/` is
// strict mode, and an assignment to a name nothing declared is a
// `ReferenceError` rather than an implicit global. One of those took the
// session-reset path down mid-way and lost the conversation with nothing
// saying why (#737).
//
// Deliberately narrow. It looks at statement-level `name = …` only: a property
// assignment carries a dot, and a declaration carries a keyword. So it cannot
// see a mistake made through `globalThis`, and it is not a substitute for a
// real checker.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

const SRC = new URL("../src/", import.meta.url);

/** Statement-level assignments to a bare identifier: `name = …`. */
function assignedBare(source) {
  const names = new Set();
  for (const match of source.matchAll(/^[ \t]*([A-Za-z_$][\w$]*)\s*=[^=>]/gm)) {
    names.add(match[1]);
  }
  return names;
}

/**
 * Whether `name` is bound anywhere in the file: declared, destructured, or
 * arriving as a parameter. Generous on purpose — a false "declared" costs a
 * missed bug of this one shape, while a false "undeclared" fails the suite on
 * working code.
 */
function bound(source, name) {
  return new RegExp(
    [
      `(?:let|const|var|function|class)\\s+${name}\\b`,
      `\\b${name}\\s*(?:,|\\})[^=]*=\\s*(?:await\\s+)?[\\w.]`,
      `function\\s*\\([^)]*\\b${name}\\b`,
      `\\(\\s*${name}\\s*(?:,|\\))`,
    ].join("|"),
  ).test(source);
}

test("no module assigns to a name it never declares", () => {
  const modules = readdirSync(SRC).filter((name) => name.endsWith(".js"));
  assert.ok(modules.length > 0, "found no modules to check");

  const orphans = [];
  for (const file of modules) {
    const source = readFileSync(new URL(file, SRC), "utf8");
    for (const name of assignedBare(source)) {
      if (!bound(source, name)) {
        orphans.push(`${file}: ${name}`);
      }
    }
  }

  assert.deepEqual(
    orphans,
    [],
    `assigned but never declared, which throws in a module (#737):\n  ${orphans.join("\n  ")}`,
  );
});
