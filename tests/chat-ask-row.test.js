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

test("the options sit beside the question, not inside it", () => {
  for (const name of ["asked", "elicited"]) {
    const body = fnBody(name);
    assert.doesNotMatch(
      body,
      /body\.append\(\s*buttons\s*\)/,
      `${name} puts the buttons inside .said, which wraps anywhere and eats spaces`,
    );
    assert.match(
      body,
      /row\.append\([^)]*\bbuttons\b/,
      `${name} has to mount the options on the row so they do not inherit .said`,
    );
  }
});

test("ask option buttons keep their words and do not shrink off the line", () => {
  const options = ruleBlock(".row.ask .options");
  assert.match(
    options,
    /flex-wrap:\s*wrap/,
    "a long Allow-and-remember option wraps onto the next line instead of crushing Yes and No",
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
