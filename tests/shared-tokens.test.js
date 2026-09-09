// Run with `node --test tests/`.
//
// ADR-0019 lets the overlay Speech bubble take the Chat panel's opaque type,
// shape and hue, and forbids it the chat's layout, chrome and alpha-over-panel
// tokens. src/shared-tokens.css is where the first list lives (#545). Two ways
// that goes wrong quietly: a value stops being shared and drifts again, or the
// second list leaks into the first.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

// Comments come out of every file, because prose about a token is not a token.
// A declaration reverted to a hand-copied literal under `/* was
// var(--shared-sans) */` is exactly the drift these tests exist to catch, and
// reading the raw text would let that comment answer for the rule it replaced.
const read = (name) =>
  readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

const shared = read("shared-tokens.css");
const chat = read("chat-ui.css");
const overlay = read("main.css");

// `--shared-x: value;` declarations.
const declarations = [...shared.matchAll(/(--shared-[\w-]+)\s*:\s*([^;]+);/g)];

test("both files read every shared token, so one edit moves both", () => {
  assert.ok(declarations.length > 0, "shared-tokens.css declares nothing");

  for (const [, token] of declarations) {
    assert.match(chat, new RegExp(`var\\(${token}\\)`), `chat-ui.css ignores ${token}`);
    assert.match(overlay, new RegExp(`var\\(${token}\\)`), `main.css ignores ${token}`);
  }
});

// Every shape an alpha can take, not just the function names: `#14171ecc` is
// one character from the panel colour that ships, and the slash form covers
// `rgb(20 23 30 / 80%)` and `oklch(... / 0.8)`. Four and eight hex digits carry
// alpha; three and six do not, and `\b` is what keeps `#14171e` out of it.
const ALPHA = /rgba|hsla|transparent|color-mix|#(?:[0-9a-f]{8}|[0-9a-f]{4})\b|\/\s*[\d.]+%?\s*\)/i;

test("nothing crosses that assumes a panel underneath it", () => {
  for (const [declaration, token, value] of declarations) {
    assert.doesNotMatch(
      value,
      ALPHA,
      `${token} carries alpha, which is an instruction for sitting on the chat panel and means nothing over a wallpaper: ${declaration.trim()}`,
    );
  }
});

test("the bubble takes no chat token", () => {
  const named = [...overlay.matchAll(/--chat-[\w-]+/g)].map((match) => match[0]);

  assert.deepEqual(named, [], `main.css reaches into the chat surface's own tokens: ${named.join(", ")}`);
});
