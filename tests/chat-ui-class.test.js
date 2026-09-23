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
