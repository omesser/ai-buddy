// #706 Step 9: Settings is only the webview. Native flags and renderers are gone.
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function read(rel) {
  return readFileSync(join(root, rel), "utf8");
}

test("Settings env flags and the surface helper are gone", () => {
  const helper = read("src-tauri/src/settings.rs");
  const main = read("src-tauri/src/main.rs");
  const platform = read("src-tauri/src/platform.rs");
  assert.doesNotMatch(helper, /AI_BUDDY_SETTINGS_NATIVE/);
  assert.doesNotMatch(helper, /AI_BUDDY_SETTINGS_WEBVIEW/);
  assert.doesNotMatch(helper, /fn settings_is_webview/);
  assert.doesNotMatch(main, /settings_is_webview/);
  assert.doesNotMatch(main, /AI_BUDDY_SETTINGS_NATIVE/);
  assert.doesNotMatch(main, /AI_BUDDY_SETTINGS_WEBVIEW/);
  assert.doesNotMatch(platform, /settings_is_webview/);
  assert.doesNotMatch(platform, /AI_BUDDY_SETTINGS_NATIVE/);
  assert.doesNotMatch(platform, /AI_BUDDY_SETTINGS_WEBVIEW/);
});

test("native Settings renderer files are gone", () => {
  for (const rel of [
    "src-tauri/src/platform/macos/settings_window.rs",
    "src-tauri/src/platform/windows/settings_window.rs",
    "src-tauri/src/platform/x11/settings_window.rs",
    "src-tauri/src/settings/move_drag.rs",
    "scripts/verify-settings-macos.sh",
    "scripts/verify-settings-linux.sh",
    "scripts/verify-settings-win.ps1",
  ]) {
    assert.equal(existsSync(join(root, rel)), false, rel);
  }
});
