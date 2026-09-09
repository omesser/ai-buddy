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

const read = (name) => readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");

const shared = read("shared-tokens.css");
const chat = read("chat-ui.css");
const overlay = read("main.css");

// `--shared-x: value;` declarations, comments stripped so a token named in
// prose is not read as one that ships.
const declarations = [
  ...shared.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/(--shared-[\w-]+)\s*:\s*([^;]+);/g),
];

test("both files read every shared token, so one edit moves both", () => {
  assert.ok(declarations.length > 0, "shared-tokens.css declares nothing");

  for (const [, token] of declarations) {
    assert.match(chat, new RegExp(`var\\(${token}\\)`), `chat-ui.css ignores ${token}`);
    assert.match(overlay, new RegExp(`var\\(${token}\\)`), `main.css ignores ${token}`);
  }
});

test("nothing crosses that assumes a panel underneath it", () => {
  for (const [declaration, token, value] of declarations) {
    assert.doesNotMatch(
      value,
      /rgba|hsla|transparent|color-mix/,
      `${token} carries alpha, which is an instruction for sitting on the chat panel and means nothing over a wallpaper: ${declaration.trim()}`,
    );
  }
});

test("the bubble takes no chat token", () => {
  const named = [
    ...overlay.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/--chat-[\w-]+/g),
  ].map((match) => match[0]);

  assert.deepEqual(named, [], `main.css reaches into the chat surface's own tokens: ${named.join(", ")}`);
});
