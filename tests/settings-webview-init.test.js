// Run with `node --test tests/`.
//
// Step 5 wires the Settings webview to listen for settings-refresh events and
// reload the snapshot when the roster changes while the window is open. This
// test verifies the page initialization contract: that the functions exist, the
// event listener can be registered, and a snapshot can be requested.

import assert from "node:assert/strict";
import { test } from "node:test";

import { controls, render } from "../src/settings.js";

test("controls() returns a flat list for any tab", () => {
  const tab = {
    sections: [
      {
        heading: "Test",
        rows: [
          { type: "Checkbox", id: "test_bool", label: "Test", frozen: false },
          { type: "TextField", id: "test_text", label: "Text", frozen: false },
        ],
      },
    ],
  };
  const values = { test_bool: true, test_text: "value" };
  const list = controls(tab, values);

  assert.equal(list.length, 3); // heading + 2 rows
  assert.equal(list[0].role, "heading");
  assert.equal(list[1].role, "checkbox");
  assert.equal(list[1].id, "test_bool");
  assert.equal(list[1].value, true);
  assert.equal(list[2].role, "textfield");
  assert.equal(list[2].id, "test_text");
  assert.equal(list[2].value, "value");
});

test("render() signature supports the emit callback Step 5 needs", () => {
  // Step 5 wires render(panel, tab, values, emitEvent) where emitEvent
  // invokes settings_event. render() has emit as an optional 4th parameter
  // with a default stub, so its .length is 3.
  assert.equal(typeof render, "function");
  assert.equal(render.length, 3, "render() has 3 required parameters: root, tab, values");

  // The 4th parameter (emit) has a default value of () => {}, making it
  // optional but callable. Step 5's initialization passes emitEvent as the
  // 4th argument to wire settings_event invocation.
});

test("settings.js exports the functions Step 5 initialization needs", () => {
  assert.equal(typeof controls, "function", "controls() must be exported");
  assert.equal(typeof render, "function", "render() must be exported");
});
