// Run with `node --test tests/`.
//
// `node --check` parses each file alone and cannot see a named import that
// the exporter dropped. The overlay is a type=module page: one missing
// binding is a SyntaxError, start() never runs, and the Character is
// invisible with nothing on the Shell's stderr.

import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const src = join(dirname(fileURLToPath(import.meta.url)), "../src");

test("overlay main.js named imports exist on their modules", async () => {
  const main = readFileSync(join(src, "main.js"), "utf8");
  const named = [
    ...main.matchAll(/import\s*\{([^}]+)\}\s*from\s*"(\.\/[^"]+)"/g),
  ];
  assert.ok(named.length > 0, "main.js has named imports to check");
  for (const [, names, spec] of named) {
    const mod = await import(pathToFileURL(join(src, spec)).href);
    for (const name of names
      .split(",")
      .map((n) => n.trim())
      .filter(Boolean)) {
      assert.equal(name in mod, true, `${spec} exports ${name}`);
    }
  }
});
