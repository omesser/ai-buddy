import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sh = readFileSync(
  join(root, "scripts/verify-settings-webview-select-macos.sh"),
  "utf8",
);

test("select-over-overlay verify summons a character and uses a mouse pick", () => {
  assert.doesNotMatch(sh, /FIDGET_SETTINGS_NATIVE=1/);
  assert.match(sh, /click-cursor\.swift/);
  assert.match(sh, /\^verbs:\.\*Summon/);
  assert.doesNotMatch(sh, /role: "combobox"/);
});
