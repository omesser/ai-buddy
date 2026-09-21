// #706 Step 8: webview is the default Settings surface; native is the opt-in.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function read(rel) {
  return readFileSync(join(root, rel), "utf8");
}

test("product code opts into native Settings, not the old webview flag", () => {
  const helper = read("src-tauri/src/settings.rs");
  const main = read("src-tauri/src/main.rs");
  const platform = read("src-tauri/src/platform.rs");
  assert.match(helper, /fn settings_is_webview/);
  assert.match(helper, /AI_BUDDY_SETTINGS_NATIVE/);
  assert.match(main, /settings_is_webview/);
  assert.match(platform, /settings_is_webview/);
  assert.doesNotMatch(platform, /AI_BUDDY_SETTINGS_WEBVIEW/);
});

test("native verify scripts pin the escape hatch", () => {
  assert.match(read("scripts/verify-settings-macos.sh"), /AI_BUDDY_SETTINGS_NATIVE=1/);
  assert.match(read("scripts/verify-settings-linux.sh"), /AI_BUDDY_SETTINGS_NATIVE=1/);
  assert.match(read("scripts/verify-settings-win.ps1"), /AI_BUDDY_SETTINGS_NATIVE\s*=\s*"1"/);
});
