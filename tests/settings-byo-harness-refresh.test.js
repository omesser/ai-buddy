import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const js = readFileSync(new URL("../src/settings.js", import.meta.url), "utf8");

// #867: changing the BYO Harness popup must use pick event (not set_text)
// so the controller returns Apply, whose refresh answer triggers loadSnapshot
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

test("picking a registration Harness keeps the previous preview until the new one arrives", async () => {
  const { foldDraft, processResponse, stageDraft } = await import("../src/settings.js");
  const values = {
    ...JSON.parse(readFileSync(new URL("./fixtures/settings-values-modelApi.json", import.meta.url), "utf8")),
    byo_token: "old-token",
  };
  const shown = (draft) => ({ ...values, ...draft });
  const fields = ["byo_snippet", "byo_steps", "byo_token"];

  let draft = stageDraft({}, "byo_harness", "hermes");
  for (const id of fields) assert.equal(shown(draft)[id], values[id], `${id} while the preview is in flight`);
  for (const id of fields) assert.notEqual(shown(draft)[id], "", id);

  draft = foldDraft(
    draft,
    processResponse({ action: "preview_byo", snippet: "hermes mcp add fidget", steps: "Run it.", token: "t" }),
  );
  assert.equal(shown(draft).byo_snippet, "hermes mcp add fidget");
  assert.equal(shown(draft).byo_harness, "hermes");
});
