// Alt-drag and Escape shipped dead once: they called a window handle the Tauri
// global does not export, so the calls threw only under a user's press while
// every unit test stayed green. Asserted on the source; the handler needs a DOM.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

const src = new URL("../src/", import.meta.url);

function webviewFiles() {
  return readdirSync(src)
    .filter((name) => name.endsWith(".js"))
    .map((name) => [name, readFileSync(new URL(name, src), "utf8")]);
}

test("nothing reaches for a window handle the Tauri global does not export", () => {
  // `webviewWindow` exports `getCurrentWebviewWindow`. `getCurrentWindow` lives
  // on the `window` module, which this app does not load, so naming it here is
  // always a TypeError waiting for a user.
  const wrong = /__TAURI__\.webviewWindow[\s\S]{0,80}?\bgetCurrentWindow\b|\bgetCurrentWindow\b[\s\S]{0,80}?__TAURI__\.webviewWindow/;
  const offenders = webviewFiles()
    .filter(([, text]) => wrong.test(text))
    .map(([name]) => name);

  assert.deepEqual(offenders, []);
});

test("every window handle comes from getCurrentWebviewWindow", () => {
  const handles = webviewFiles().filter(([, text]) => text.includes("__TAURI__.webviewWindow"));

  assert.ok(handles.length > 0, "some file asks the Tauri global for a window");
  for (const [name, text] of handles) {
    assert.match(
      text,
      /getCurrentWebviewWindow/,
      `${name} asks webviewWindow for a handle but never names getCurrentWebviewWindow`,
    );
  }
});
