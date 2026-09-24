// Behavior test for Chat UI class application (Architect Soft #1):
// - normalizeChatUi allowlists known designs and maps unknowns to minimal
// - Root class becomes chat-ui-{design} on change and reopen

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const chatJs = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");

// Extract normalizeChatUi function from chat.js source
const normalizeChatUiMatch = chatJs.match(
  /function normalizeChatUi\(value\) \{[\s\S]*?return allowed\.includes\(value\) \? value : "minimal";[\s\S]*?\}/,
);

if (!normalizeChatUiMatch) {
  throw new Error("normalizeChatUi function not found in chat.js");
}

const normalizeChatUi = new Function(`
  ${normalizeChatUiMatch[0]}
  return normalizeChatUi;
`)();

test("normalizeChatUi allowlists minimal, terminal, glass", () => {
  assert.equal(normalizeChatUi("minimal"), "minimal");
  assert.equal(normalizeChatUi("terminal"), "terminal");
  assert.equal(normalizeChatUi("glass"), "glass");
});

test("normalizeChatUi maps unknowns to minimal", () => {
  assert.equal(normalizeChatUi("unknown"), "minimal");
  assert.equal(normalizeChatUi("malicious<script>"), "minimal");
  assert.equal(normalizeChatUi(""), "minimal");
  assert.equal(normalizeChatUi(null), "minimal");
  assert.equal(normalizeChatUi(undefined), "minimal");
});

test("chat.js uses normalizeChatUi on opening.chat_ui", () => {
  // Verify attached() calls normalizeChatUi on opening.chat_ui
  const attachedMatch = chatJs.match(
    /const chatUi = normalizeChatUi\(opening\.chat_ui \|\| "minimal"\)/,
  );
  assert.ok(
    attachedMatch,
    "attached() should call normalizeChatUi on opening.chat_ui",
  );
});

test("chat.js uses normalizeChatUi on chat-ui event payload", () => {
  // Verify the chat-ui event listener calls normalizeChatUi
  const eventMatch = chatJs.match(
    /const chatUi = normalizeChatUi\(payload\);[\s\S]*?html\.classList\.add\(`chat-ui-\$\{chatUi\}`\)/,
  );
  assert.ok(
    eventMatch,
    "chat-ui event listener should call normalizeChatUi on payload",
  );
});

test("chat.js applies chat-ui class early to prevent FOUC", () => {
  // Verify the class is applied right after getting opening, before attached()
  const fouc = chatJs.match(
    /const opening = await invoke\("chat_opening"[\s\S]*?const html = document\.documentElement;[\s\S]*?const chatUi = normalizeChatUi\(opening\.chat_ui[\s\S]*?html\.classList\.add\(`chat-ui-\$\{chatUi\}`\);[\s\S]*?attached\(opening\)/,
  );
  assert.ok(
    fouc,
    "chat-ui class should be applied immediately after receiving opening, before attached()",
  );
});

test("attached() swaps DOM classes when applying saved chat_ui", () => {
  // Verify attached() removes all three possible classes before adding the chosen one
  const attachedFn = chatJs.match(
    /function attached\(opening\) \{[\s\S]*?\}/,
  );
  assert.ok(attachedFn, "attached() function should exist");

  const classSwap = chatJs.match(
    /html\.classList\.remove\("chat-ui-minimal", "chat-ui-terminal", "chat-ui-glass"\);[\s\S]*?html\.classList\.add\(`chat-ui-\$\{chatUi\}`\);/,
  );
  assert.ok(
    classSwap,
    "attached() should remove all three chat-ui classes then add the chosen one",
  );
});

test("chat-ui event listener swaps DOM classes on live change", () => {
  // Verify the event listener removes all three possible classes before adding new one
  const hasListener = chatJs.includes('await listen(\n    "chat-ui",');
  assert.ok(hasListener, "chat-ui event listener should exist");

  // Check that the listener has the class swap logic
  const listenerStart = chatJs.indexOf('await listen(\n    "chat-ui",');
  const listenerEnd = chatJs.indexOf("{ target: chat.label },", listenerStart) + 50;
  const listenerCode = chatJs.slice(listenerStart, listenerEnd);

  const hasRemove = listenerCode.includes('html.classList.remove("chat-ui-minimal", "chat-ui-terminal", "chat-ui-glass")');
  const hasAdd = listenerCode.includes("html.classList.add(`chat-ui-${chatUi}`)");
  
  assert.ok(
    hasRemove,
    "chat-ui event should remove all three chat-ui classes",
  );
  assert.ok(
    hasAdd,
    "chat-ui event should add the chosen chat-ui class",
  );
});

test("cold-open applies saved chat_ui class before window becomes visible", () => {
  // Verify start() applies the class immediately after getting opening, before calling attached()
  const startFn = chatJs.match(
    /async function start\(\) \{[\s\S]*?\n\}/,
  );
  assert.ok(startFn, "start() function should exist");

  const coldOpen = startFn[0].match(
    /const opening = await invoke\("chat_opening"[\s\S]*?const html = document\.documentElement;[\s\S]*?const chatUi = normalizeChatUi\(opening\.chat_ui[\s\S]*?html\.classList\.add\(`chat-ui-\$\{chatUi\}`\);[\s\S]*?showWho\(opening\);[\s\S]*?attached\(opening\);/,
  );
  assert.ok(
    coldOpen,
    "start() should apply chat-ui class immediately after receiving opening to prevent FOUC",
  );
});
