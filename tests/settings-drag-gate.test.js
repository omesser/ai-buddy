// Alt-drag gate: modifier on background begins drag; controls keep the press.
// Ports move_drag.rs via .closest("input, select, button, summary, pre").

import { describe, test } from "node:test";
import { strict as assert } from "node:assert";
import { shouldBeginDrag } from "../src/settings.js";

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
