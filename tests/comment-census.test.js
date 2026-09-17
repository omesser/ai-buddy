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

const spawnCensus = (target, files) => {
  const tmpDir = mkdtempSync(join(tmpdir(), "census-test-"));
  for (const [path, content] of Object.entries(files)) {
    writeFileSync(join(tmpDir, path), content);
  }

  const result = spawnSync("python3", [censusScript, join(tmpDir, target)], {
    encoding: "utf8",
  });

  return {
    stdout: result.stdout,
    stderr: result.stderr,
    status: result.status,
  };
};

const runCensus = (files) => spawnCensus(".", files);

const runCensusFile = (filename, content) => spawnCensus(filename, { [filename]: content });

const field = (stdout, name) => {
  const match = stdout.match(new RegExp(`^ {2}${name}: ([\\d,]+)`, "m"));
  assert.ok(match, `missing field ${name} in:\n${stdout}`);
  return Number(match[1].replace(/,/g, ""));
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

test("census ignores HTML entities as issue citations", () => {
  const result = runCensus({
    "test.js": `// Entity &#106; is not an issue
// Neither is &#39; or &#34;
function main() {}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /citing issue.*0/i);
});

test("census still detects real issue citations after stripping entities", () => {
  const result = runCensus({
    "test.js": `// Entity &#106; mixed with issue #371
function main() {}`,
  });

  assert.equal(result.status, 0);
  assert.match(result.stdout, /citing issue.*1/i);
});

test("census counts Swift // and /* */ comment blocks", () => {
  const result = runCensusFile(
    "sample.swift",
    `// header
import Foundation
/* block
   still */
func f() {}`,
  );

  assert.equal(result.status, 0);
  assert.equal(field(result.stdout, "Comment lines"), 3);
  assert.equal(field(result.stdout, "Code lines"), 2);
  assert.equal(field(result.stdout, "Blocks"), 2);
});

test("census counts PowerShell # and <# #> comment blocks", () => {
  const result = runCensusFile(
    "sample.ps1",
    `# header
Write-Host "hi"
<#
.SYNOPSIS
note
#>
Get-Date`,
  );

  assert.equal(result.status, 0);
  assert.equal(field(result.stdout, "Comment lines"), 5);
  assert.equal(field(result.stdout, "Code lines"), 2);
  assert.equal(field(result.stdout, "Blocks"), 2);
});

test("census counts .cjs C-style comment blocks", () => {
  const result = runCensusFile(
    "sample.cjs",
    `// stub
module.exports = 1;
/* x */`,
  );

  assert.equal(result.status, 0);
  assert.equal(field(result.stdout, "Comment lines"), 2);
  assert.equal(field(result.stdout, "Code lines"), 1);
  assert.equal(field(result.stdout, "Blocks"), 2);
});

test("census directory scan finds Swift, PowerShell, and cjs files", () => {
  const result = runCensus({
    "sample.swift": "// a\nlet x = 1\n",
    "sample.ps1": "# a\nWrite-Host 1\n",
    "sample.cjs": "// a\nmodule.exports = 1\n",
  });

  assert.equal(result.status, 0);
  assert.equal(field(result.stdout, "Files"), 3);
  assert.equal(field(result.stdout, "Comment lines"), 3);
  assert.equal(field(result.stdout, "Blocks"), 3);
});
