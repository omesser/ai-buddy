import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const js = readFileSync(new URL("../src/settings.js", import.meta.url), "utf8");

// #867: changing the BYO Harness popup must use pick event (not set_text)
// so the controller returns ApplyAndRefresh, which triggers loadSnapshot
// and regenerates the snippet below.
test("non-batched Popup rows emit pick on change, not set_text", () => {
  // Match the Popup case that checks !row.batched and emits an event
  const popupCaseMatch = js.match(/case "Popup"[\s\S]{1,500}?return labelled/);
  assert.ok(popupCaseMatch, "Popup case must exist");

  const popupCase = popupCaseMatch[0];
  assert.match(
    popupCase,
    /emit\(\{ pick: row\.id, value: select\.value \}\)/,
    "non-batched Popup rows must emit pick with row.id and select.value"
  );
  assert.doesNotMatch(
    popupCase,
    /emit\(\{ set_text: row\.id/,
    "non-batched Popup rows must not emit set_text"
  );
});
