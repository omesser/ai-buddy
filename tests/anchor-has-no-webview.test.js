// The panel anchor is a taskbar button. A webview there is a second renderer
// process for a page that never draws. `build_anchor_window` is the only
// place that button is made.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const main = readFileSync(join(root, "src-tauri/src/main.rs"), "utf8");

function buildAnchorWindow() {
  const start = main.indexOf("fn build_anchor_window");
  assert.notEqual(start, -1, "build_anchor_window exists");
  const next = main.indexOf("\nfn ", start + 1);
  return main.slice(start, next === -1 ? undefined : next);
}

test("the panel anchor is a window with no document", () => {
  const body = buildAnchorWindow();
  assert.match(body, /WindowBuilder::new/);
  assert.doesNotMatch(body, /WebviewWindowBuilder/);
  assert.doesNotMatch(body, /WebviewUrl/);
});
