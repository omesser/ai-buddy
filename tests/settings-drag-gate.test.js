// Alt-drag gate: modifier on background begins drag; controls keep the press.
// `.closest("input, select, button, summary, pre")` is the page's hit test.

import { describe, test } from "node:test";
import { strict as assert } from "node:assert";
import { CONTROL_SELECTOR, shouldBeginDrag } from "../src/settings.js";

function makeEvent(altKey, matchesControl) {
  return {
    altKey,
    target: {
      closest: (selector) => (matchesControl ? {} : null),
    },
  };
}

describe("alt-drag gate", () => {
  test("modifier on background begins drag", () => {
    const event = makeEvent(true, false);
    assert.equal(shouldBeginDrag(event), true);
  });

  test("background without modifier does not begin drag", () => {
    const event = makeEvent(false, false);
    assert.equal(shouldBeginDrag(event), false);
  });

  test("modifier on control does not begin drag", () => {
    const event = makeEvent(true, true);
    assert.equal(shouldBeginDrag(event), false);
  });

  test("control without modifier does not begin drag", () => {
    const event = makeEvent(false, true);
    assert.equal(shouldBeginDrag(event), false);
  });
});

// A target that answers closest() the way the DOM would for a single element:
// its tag matches when the selector list names it.
function tagged(tag) {
  return {
    altKey: true,
    target: {
      closest: (selector) =>
        selector
          .split(",")
          .map((part) => part.trim())
          .includes(tag)
          ? {}
          : null,
    },
  };
}

describe("the gate covers every control this page renders", () => {
  // A checkbox row is a <label> wrapping its input, so the label's box is the
  // whole row. Alt-dragging it used to move the window.
  test("a checkbox row's label keeps the press", () => {
    assert.equal(shouldBeginDrag(tagged("label")), false);
  });

  test("a Multiline textarea keeps the press, so the drag selects text", () => {
    assert.equal(shouldBeginDrag(tagged("textarea")), false);
  });

  test("a heading is still background", () => {
    assert.equal(shouldBeginDrag(tagged("h1")), true);
  });

  test("the selector names every element the renderer builds a control from", () => {
    for (const tag of ["input", "textarea", "select", "button", "summary", "pre", "label"]) {
      assert.ok(CONTROL_SELECTOR.includes(tag), `${tag} is missing from the selector`);
    }
  });
});
