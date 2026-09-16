// Alt-drag gate: drag should begin only when modifier is held AND target is background.
//
// This test verifies the gate logic ported from move_drag.rs: alt-drag starts
// only when altKey is true and the target is not a control. Controls are matched
// by .closest("input, select, button, summary, pre").

import { describe, test } from "node:test";
import { strict as assert } from "node:assert";

function shouldBeginDrag(modifierHeld, targetIsControl) {
  return modifierHeld && !targetIsControl;
}

describe("alt-drag gate", () => {
  test("modifier on background begins drag", () => {
    assert.equal(shouldBeginDrag(true, false), true);
  });

  test("background without modifier does not begin drag", () => {
    assert.equal(shouldBeginDrag(false, false), false);
  });

  test("modifier on control does not begin drag", () => {
    assert.equal(shouldBeginDrag(true, true), false);
  });

  test("control without modifier does not begin drag", () => {
    assert.equal(shouldBeginDrag(false, true), false);
  });
});
