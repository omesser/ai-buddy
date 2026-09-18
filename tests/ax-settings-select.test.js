import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const ax = readFileSync(join(root, "scripts/ax-settings.swift"), "utf8");
const sh = readFileSync(
  join(root, "scripts/verify-settings-webview-select-macos.sh"),
  "utf8",
);

test("ax-settings can leave a <select> menu open for a still", () => {
  assert.match(ax, /case "open-popup":/);
  assert.match(ax, /verify-settings-webview-select-macos\.sh/);
});

test("select-over-overlay verify uses the webview flag, summon, and a mouse pick", () => {
  assert.match(sh, /AI_BUDDY_SETTINGS_WEBVIEW=1/);
  assert.match(sh, /click-cursor\.swift/);
  assert.match(sh, /\^verbs:\.\*Summon/);
  assert.doesNotMatch(sh, /role: "combobox"/);
});
