// The consent row is a question plus the options as buttons. Nesting the
// buttons inside `.said` inherits `overflow-wrap: anywhere`, which in the
// 420-point Chat window shrinks Yes/No to a single character. #908.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");

function ruleBlock(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = css.match(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`));
  assert.ok(match, `${selector} has no rule of its own`);
  return match[1];
}

function fnBody(name) {
  const at = js.indexOf(`function ${name}(`);
  assert.notEqual(at, -1, `${name} is missing`);
  const from = js.indexOf("{", at);
  let depth = 0;
  for (let i = from; i < js.length; i += 1) {
    if (js[i] === "{") {
      depth += 1;
    } else if (js[i] === "}") {
      depth -= 1;
      if (depth === 0) {
        return js.slice(from, i + 1);
      }
    }
  }
  assert.fail(`${name} has no matching close`);
}

test("asked and elicited still mount the options under the question", () => {
  for (const name of ["asked", "elicited"]) {
    const src = fnBody(name);
    assert.match(
      src,
      /body\.append\(\s*buttons\s*\)/,
      `${name} keeps the answers under the question, not as a third row sibling`,
    );
  }
});

test("ask options stack one per line and keep their words", () => {
  const options = ruleBlock(".row.ask .options");
  assert.match(
    options,
    /flex-direction:\s*column/,
    "the Harness lists one answer per line; a wrapping row drops No under Yes",
  );

  const button = ruleBlock(".row.ask button");
  assert.match(
    button,
    /min-width:\s*min-content/,
    "Yes and No must not shrink narrower than the word they say",
  );
  assert.match(
    button,
    /overflow-wrap:\s*normal/,
    "option copy wraps on words, not between Y and es",
  );
});
