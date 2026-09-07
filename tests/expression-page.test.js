// Honesty of the overlay-expression Generated page (ADR-0011). Arithmetic
// already lives in bubble.js; this file only checks that the published page
// loads that module rather than restating it.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

import { bubbleDuration } from "../src/bubble.js";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SCRIPT = join(ROOT, "scripts", "make-expression-page.py");
const FORMULA = /900\s*\+\s*55/;

function python(args, options = {}) {
  return execFileSync("python3", [SCRIPT, ...args], {
    cwd: ROOT,
    encoding: "utf8",
    ...options,
  });
}

test("the generator self-check passes", () => {
  const out = python(["--self-check"]);
  assert.match(out, /self-check:/);
});

test("the published page loads bubble.js and calls its machine", () => {
  const outDir = mkdtempSync(join(tmpdir(), "expression-page-"));
  try {
    python(["--out", outDir]);
    const html = readFileSync(join(outDir, "expression.html"), "utf8");
    assert.match(html, /type="module"/);
    assert.match(html, /from\s+["']\.\/bubble\.js["']/);
    assert.match(html, /\bbubbleDuration\s*\(/);
    assert.match(html, /\bcreateBubbleMachine\s*\(/);
    assert.doesNotMatch(html, FORMULA, "reading time must come from bubble.js");
    assert.doesNotMatch(
      html,
      /<meta\s+name="robots"\s+content="noindex"/i,
      "Generated pages stay indexed",
    );
    assert.match(html, /Generated from src\/bubble\.js at deploy/);
    assert.doesNotMatch(
      html,
      /THINKING_GRACE_MS\s*=\s*250|THINKING_MIN_HOLD_MS\s*=\s*600/,
      "grace and hold are imported, not restated",
    );

    const css = readFileSync(join(outDir, "bubble.css"), "utf8");
    assert.match(css, /\.bubble\.visible/);
    assert.match(css, /\[data-mode="speech"\]/);
    assert.match(css, /\[data-mode="thinking"\]/);
    assert.match(css, /\.thinking-dots/);
    assert.doesNotMatch(css, /background:\s*transparent/, "overlay fullscreen rules stay out");

    const copied = readFileSync(join(outDir, "bubble.js"), "utf8");
    assert.equal(copied, readFileSync(join(ROOT, "src", "bubble.js"), "utf8"));
  } finally {
    rmSync(outDir, { recursive: true, force: true });
  }
});

test("the copied module is the same bubbleDuration the tests already own", async () => {
  const outDir = mkdtempSync(join(tmpdir(), "expression-bubble-"));
  try {
    python(["--out", outDir]);
    const copied = await import(pathToFileURL(join(outDir, "bubble.js")).href);
    const sample = "a".repeat(30);
    assert.equal(copied.bubbleDuration(sample), bubbleDuration(sample));
    assert.equal(typeof copied.createBubbleMachine, "function");
  } finally {
    rmSync(outDir, { recursive: true, force: true });
  }
});
