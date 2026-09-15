// Run with `node --test spike/settings-webview/`.
//
// What scripts/verify-settings-macos.sh asks an AX dump, asked of the flat
// control list instead: section order, labels, and whether the HTTP rows are
// live or frozen for the source in force.

import assert from "node:assert/strict";
import { test } from "node:test";

import { STATES } from "./ai-tab.js";
import { controls } from "./settings.js";

const headings = (list) => list.filter((c) => c.role === "heading").map((c) => c.label);
const byId = (list, id) => list.find((c) => c.id === id);

test("AI tab section order is AI, AI source, Model / API, Last user turn", () => {
  for (const state of Object.values(STATES)) {
    assert.deepEqual(headings(controls(state.tab, state.view)), ["AI", "AI source", "Model / API", "Last user turn"]);
  }
});

test("HTTP rows are live when Model API drives", () => {
  const { tab, view } = STATES.modelApi;
  const list = controls(tab, view);
  for (const id of ["director_base_url", "director_model", "director_api_key", "director_base_url_pick", "clear_key"]) {
    assert.equal(byId(list, id).frozen, false, id);
  }
  assert.equal(byId(list, "harness").value, "Model API");
});

test("HTTP rows freeze when a Harness drives, and the picker says which", () => {
  const { tab, view } = STATES.harnessDriving;
  const list = controls(tab, view);
  for (const id of ["director_base_url", "director_model", "director_api_key", "director_base_url_pick", "clear_key"]) {
    assert.equal(byId(list, id).frozen, true, id);
  }
  assert.equal(byId(list, "harness").value, "Harness · claude");
  assert.equal(byId(list, "director_apply").frozen, false);
});

test("every labelled row keeps its label across states", () => {
  const a = controls(STATES.modelApi.tab, STATES.modelApi.view).map((c) => [c.role, c.id, c.label]);
  const b = controls(STATES.harnessDriving.tab, STATES.harnessDriving.view).map((c) => [c.role, c.id, c.label]);
  assert.deepEqual(a, b);
});
