// Run with `node --test tests/`.
//
// #545: opaque panel tokens the Speech bubble may share with the default Chat
// UI live in one snippet. Deleting the import or copying the literals back
// into either stylesheet is the failure this file exists to catch.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SNIPPET_FILE = "chat-shared.css";
const IMPORT = new RegExp(
  String.raw`@import\s+(?:url\(\s*)?["'](?:\./)?${SNIPPET_FILE}["']`,
);

const SHARED = [
  "--shared-panel",
  "--shared-ink",
  "--shared-accent",
  "--shared-radius",
  "--shared-type-size",
  "--shared-type-leading",
];

const CHROME = [
  "--chat-fill",
  "--chat-fill-dim",
  "--chat-fill-soft",
  "--chat-rule",
  "--chat-line",
  "--chat-accent-soft",
];

function src(name) {
  return join(ROOT, "src", name);
}

function read(name) {
  return readFileSync(src(name), "utf8");
}

test("a shared snippet exists and both stylesheets import it", () => {
  assert.equal(existsSync(src(SNIPPET_FILE)), true, `src/${SNIPPET_FILE} is missing`);
  assert.match(read("chat-ui.css"), IMPORT, "chat-ui.css dropped the snippet import");
  assert.match(read("main.css"), IMPORT, "main.css dropped the snippet import");
  assert.doesNotMatch(
    read("chat-ui.css"),
    /@import[^;]*chat-ui\.css/,
    "chat-ui.css must not import itself",
  );
});

// Independent of chat-ui.css: these are the default Chat panel values the
// bubble already shows. Duplicating them in either stylesheet (or omitting
// them here) is the drift #545 exists to stop.
test("the snippet is the single source both surfaces read", () => {
  const snippet = read(SNIPPET_FILE);
  const chat = read("chat-ui.css");
  const overlay = read("main.css");

  for (const name of SHARED) {
    assert.match(snippet, new RegExp(`${name}\\s*:`), `${name} is not declared in the snippet`);
  }

  assert.match(snippet, /--shared-panel:\s*#14171e\b/);
  assert.match(snippet, /--shared-ink:\s*#e8ebf2\b/);
  assert.match(snippet, /--shared-accent:\s*#5cc9b5\b/);
  assert.doesNotMatch(
    snippet,
    /--shared-ring\s*:/,
    "Chat has no opaque ring token; the bubble edge is ink, as it was before the snippet",
  );
  assert.match(snippet, /--shared-radius:\s*12px\b/);
  assert.match(snippet, /--shared-type-size:\s*13\.5px\b/);
  assert.match(snippet, /--shared-type-leading:\s*1\.55\b/);

  assert.match(chat, /--chat-panel:\s*var\(--shared-panel\)/);
  assert.match(chat, /--chat-ink:\s*var\(--shared-ink\)/);
  assert.match(chat, /--chat-accent:\s*var\(--shared-accent\)/);
  assert.match(chat, /--chat-radius:\s*var\(--shared-radius\)/);
  assert.match(chat, /font-size:\s*var\(--shared-type-size\)/);
  assert.match(chat, /line-height:\s*var\(--shared-type-leading\)/);

  assert.match(overlay, /--bubble-panel:\s*var\(--shared-panel\)/);
  assert.match(overlay, /--bubble-ink:\s*var\(--shared-ink\)/);
  assert.match(overlay, /--bubble-accent:\s*var\(--shared-accent\)/);
  assert.match(overlay, /border:\s*2px solid var\(--shared-ink\)/);
  assert.match(overlay, /border-radius:\s*var\(--shared-radius\)/);
  assert.match(overlay, /font-size:\s*var\(--shared-type-size\)/);
  assert.match(overlay, /line-height:\s*var\(--shared-type-leading\)/);

  assert.doesNotMatch(
    overlay,
    /--bubble-panel:\s*#14171e/,
    "overlay still copies the panel fill by hand",
  );
  assert.doesNotMatch(
    chat,
    /\.chat-ui-minimal[\s\S]*--chat-panel:\s*#14171e/,
    "Chat UI still copies the panel fill by hand",
  );
});

test("the overlay does not load Chat UI chrome or layout", () => {
  const html = readFileSync(join(ROOT, "src", "index.html"), "utf8");
  assert.match(html, /href="main\.css"/);
  assert.doesNotMatch(html, /chat-ui\.css/);
  assert.equal((html.match(/<link\s+rel="stylesheet"/g) ?? []).length, 1);

  assert.doesNotMatch(read("main.css"), /@import[^;]*chat-ui\.css/);

  const snippet = read(SNIPPET_FILE);
  const overlay = read("main.css");
  for (const token of CHROME) {
    const pattern = new RegExp(token.replaceAll("-", "\\-"));
    assert.doesNotMatch(snippet, pattern, `snippet must not carry ${token}`);
    assert.doesNotMatch(overlay, pattern, `overlay must not pick up ${token}`);
  }
  assert.doesNotMatch(snippet, /rgba\(\s*255\s*,\s*255\s*,\s*255/);
  assert.doesNotMatch(snippet, /\.tab\b|\.composer\b|\.tb\b/);
});
