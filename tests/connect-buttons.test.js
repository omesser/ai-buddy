// The landing surface is the only way to pick a Harness without opening
// Settings, and its buttons are hand-written HTML. HARNESS_PRESETS in form.rs
// is where named presets are listed; nothing else links the two, so a new preset
// could land with no button. (harness::launch has additional cases for custom argv.)

import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";

const form = readFileSync(
  new URL("../src-tauri/src/settings/form.rs", import.meta.url),
  "utf8",
);
const html = readFileSync(new URL("../src/chat.html", import.meta.url), "utf8");

// HARNESS_PRESETS is the source of truth for named harnesses that appear in the UI.
// The launch() function in harness.rs has additional cases for custom argv support.
function namedPresets() {
  const from = form.indexOf("pub const HARNESS_PRESETS:");
  assert.notEqual(from, -1, "form.rs still defines HARNESS_PRESETS");
  const to = form.indexOf("];", from);
  assert.notEqual(to, -1, "HARNESS_PRESETS array still closes with ];");
  return [
    ...form.slice(from, to).matchAll(/^\s*"([a-z][a-z0-9-]*)"/gm),
  ].map((m) => m[1]);
}

function connectButtons() {
  return [
    ...html.matchAll(/class="connect-btn" data-harness="([a-z0-9-]+)"/g),
  ].map((m) => m[1]);
}

test("every named Harness has a Connect button, and every button a Harness", () => {
  const named = namedPresets();
  const buttons = connectButtons();

  // Both parses read markup that is free to be reformatted. A zero or
  // near-zero count means the pattern stopped matching, not that the sets
  // agree, so refuse to pass vacuously.
  assert.ok(
    named.length >= 4,
    `parsed only ${named.length} names out of HARNESS_PRESETS — the array moved`,
  );
  assert.ok(
    buttons.length >= 4,
    `parsed only ${buttons.length} \`.connect-btn\` elements out of chat.html — the markup moved`,
  );
  assert.ok(named.includes("claude"), "`claude` is still a named preset");

  const missing = named.filter((name) => !buttons.includes(name));
  assert.deepEqual(
    missing,
    [],
    `HARNESS_PRESETS names ${missing.join(", ")} but src/chat.html ships no Connect button for ${missing.length > 1 ? "them" : "it"}`,
  );

  const stray = buttons.filter((name) => !named.includes(name));
  assert.deepEqual(
    stray,
    [],
    `src/chat.html has a Connect button for ${stray.join(", ")}, which HARNESS_PRESETS does not name`,
  );

  assert.equal(
    new Set(buttons).size,
    buttons.length,
    "each Harness has exactly one Connect button",
  );
});
