// Run with `node --test tests/`.
//
// Tests for scripts/comment-census.py behavior.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

const censusScript = join(import.meta.dirname, "..", "scripts", "comment-census.py");

const runCensus = (files) => {
  const tmpDir = mkdtempSync(join(tmpdir(), "census-test-"));
  for (const [path, content] of Object.entries(files)) {
    writeFileSync(join(tmpDir, path), content);
  }

  const result = spawnSync("python3", [censusScript, tmpDir], {
    encoding: "utf8",
  });

  return {
    stdout: result.stdout,
    stderr: result.stderr,
    status: result.status,
  };
};

test("census counts Rust line comments", () => {
  const result = runCensus({
    "test.rs": `fn main() {
    // A comment
    println!("hello");
}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /comment.*1/i);
  assert.match(result.stdout, /code.*3/i);
});

test("census counts Rust doc comments", () => {
  const result = runCensus({
    "test.rs": `/// Doc comment line 1
/// Doc comment line 2
fn main() {}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /comment.*2/i);
});

test("census counts comment blocks", () => {
  const result = runCensus({
    "test.rs": `// Block line 1
// Block line 2
// Block line 3
// Block line 4

fn main() {
    // Single line
    println!("hello");
}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, />3/);
});

test("census detects issue references", () => {
  const result = runCensus({
    "test.rs": `// This fixes #123
fn main() {
    // Also see #456
    println!("hello");
}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /issue|ref/i);
  assert.match(result.stdout, /2/);
});

test("census detects ADR references", () => {
  const result = runCensus({
    "test.rs": `// Per ADR-0001
fn main() {}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /adr/i);
  assert.match(result.stdout, /1/);
});

test("census handles JavaScript comments", () => {
  const result = runCensus({
    "test.js": `// Single line
/* Multi
   line
   comment */
function main() {}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /comment.*4/i);
});
