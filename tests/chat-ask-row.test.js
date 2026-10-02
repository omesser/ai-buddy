// The consent row: what an ask says, one element per part, with the options
// as buttons under it. The DOM shape is asserted on a stand-in document that
// refuses innerHTML, because every word of an ask is untrusted.

import assert from "node:assert/strict";
import { test } from "node:test";

import { drawAskDetails } from "../src/chat-ask-row.js";

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
