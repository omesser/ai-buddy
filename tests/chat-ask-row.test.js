// The consent row: what an ask says, one element per part, with the options
// as buttons under it. The DOM shape is asserted on a stand-in document that
// refuses innerHTML, because every word of an ask is untrusted.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { drawAskDetails } from "../src/chat-ask-row.js";

const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");

function ruleBlock(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = css.match(new RegExp(`(?:^|\\n)\\s*${escaped}\\s*\\{([^}]+)\\}`));
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

class Element {
  constructor(tagName, ownerDocument) {
    this.tagName = tagName.toUpperCase();
    this.ownerDocument = ownerDocument;
    this.className = "";
    this.children = [];
    this.textContent = "";
  }

  set innerHTML(_) {
    throw new Error("untrusted text must not become HTML");
  }

  append(...children) {
    this.children.push(...children);
  }
}

const document = {
  createElement(tagName) {
    return new Element(tagName, this);
  },
};

function drawn(ask) {
  const body = document.createElement("div");
  drawAskDetails(body, ask);
  return body.children.map(({ tagName, className, textContent }) => ({
    tagName,
    className,
    textContent,
  }));
}

test("an execute ask draws its command as code and its kind and paths as metadata", () => {
  assert.deepEqual(
    drawn({
      title: "Open <img src=x>",
      kind: "execute",
      content: ["open -a Calculator && <script>alert(1)</script>"],
      input: { command: "not displayed twice" },
      locations: ["/tmp/<report>"],
    }),
    [
      { tagName: "DIV", className: "ask-title", textContent: "Open <img src=x>" },
      {
        tagName: "CODE",
        className: "ask-code",
        textContent: "open -a Calculator && <script>alert(1)</script>",
      },
      { tagName: "DIV", className: "ask-metadata", textContent: "execute · /tmp/<report>" },
    ],
  );
});

test("a prose question stays prose, and argument fallback is code", () => {
  assert.deepEqual(
    drawn({ title: "Question", kind: "other", content: ["Which branch? <b>main</b>"] }),
    [
      { tagName: "DIV", className: "ask-title", textContent: "Question" },
      { tagName: "DIV", className: "ask-prose", textContent: "Which branch? <b>main</b>" },
    ],
  );
  assert.deepEqual(
    drawn({ title: "Run", kind: "execute", content: [], input: { command: "pwd" } }),
    [
      { tagName: "DIV", className: "ask-title", textContent: "Run" },
      { tagName: "CODE", className: "ask-code", textContent: "command: pwd" },
      { tagName: "DIV", className: "ask-metadata", textContent: "execute" },
    ],
  );
});

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
