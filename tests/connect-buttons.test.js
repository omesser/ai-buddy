// Run with `node --test tests/`.
//
// The landing surface is the only way to pick a Harness without opening
// Settings, and its buttons are hand-written HTML. `harness::launch` is where
// a Harness is actually named. Nothing links the two, so a seventh named row
// lands with no button and nobody notices (#675). This test is that link.

import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";

const rust = readFileSync(
  new URL("../src-tauri/src/harness.rs", import.meta.url),
  "utf8",
);
const html = readFileSync(new URL("../src/chat.html", import.meta.url), "utf8");

// `launch`'s match runs from the signature to the `custom =>` escape hatch.
// Slicing there keeps `login_hint`, which spells its arms the same way, out.
function launchTable() {
  const from = rust.indexOf("pub fn launch(");
  assert.notEqual(from, -1, "harness.rs still defines `pub fn launch(`");
  const to = rust.indexOf("custom =>", from);
  assert.notEqual(to, -1, "`launch`'s match still ends at the `custom =>` arm");
  return [
    ...rust.slice(from, to).matchAll(/^\s*"([a-z][a-z0-9-]*)" =>/gm),
  ].map((m) => m[1]);
}

function connectButtons() {
  return [
    ...html.matchAll(/class="connect-btn" data-harness="([a-z0-9-]+)"/g),
  ].map((m) => m[1]);
}

test("every named Harness has a Connect button, and every button a Harness", () => {
  const named = launchTable();
  const buttons = connectButtons();

  // Both parses read markup that is free to be reformatted. A zero or
  // near-zero count means the pattern stopped matching, not that the sets
  // agree, so refuse to pass vacuously.
  assert.ok(
    named.length >= 4,
    `parsed only ${named.length} names out of \`launch\` — the match arms moved`,
  );
  assert.ok(
    buttons.length >= 4,
    `parsed only ${buttons.length} \`.connect-btn\` elements out of chat.html — the markup moved`,
  );
  assert.ok(named.includes("claude"), "`claude` is still a named Harness");

  const missing = named.filter((name) => !buttons.includes(name));
  assert.deepEqual(
    missing,
    [],
    `\`harness::launch\` names ${missing.join(", ")} but src/chat.html ships no Connect button for ${missing.length > 1 ? "them" : "it"}`,
  );

  const stray = buttons.filter((name) => !named.includes(name));
  assert.deepEqual(
    stray,
    [],
    `src/chat.html has a Connect button for ${stray.join(", ")}, which \`harness::launch\` does not name`,
  );

  assert.equal(
    new Set(buttons).size,
    buttons.length,
    "each Harness has exactly one Connect button",
  );
});
