// Behavior test for Chat UI class application (Architect Soft #1):
// - normalizeChatUi allowlists known designs and maps unknowns to minimal
// - applyChatUiClass swaps DOM classes on reopen and live change

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const chatJs = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");

// Mock classList that behaves like the real DOM API
class MockClassList {
  constructor() {
    this.classes = new Set();
  }

  add(className) {
    this.classes.add(className);
  }

  remove(...classNames) {
    for (const className of classNames) {
      this.classes.delete(className);
    }
  }

  contains(className) {
    return this.classes.has(className);
  }

  get length() {
    return this.classes.size;
  }
}

// Create a test environment with mocked DOM
function setupTestEnvironment() {
  const mockRoot = {
    classList: new MockClassList(),
  };

  // Extract and evaluate the helper functions from chat.js
  // We need normalizeChatUi and applyChatUiClass
  const helperCode = chatJs
    .match(/function normalizeChatUi\(value\) \{[\s\S]*?\n\}/)[0] +
    "\n" +
    chatJs.match(/function applyChatUiClass\(root, design\) \{[\s\S]*?\n\}/)[0];

  const helpers = new Function(
    helperCode + "\nreturn { normalizeChatUi, applyChatUiClass };",
  )();

  return { mockRoot, helpers };
}

test("normalizeChatUi allowlists minimal, terminal, glass", () => {
  const { helpers } = setupTestEnvironment();
  assert.equal(helpers.normalizeChatUi("minimal"), "minimal");
  assert.equal(helpers.normalizeChatUi("terminal"), "terminal");
  assert.equal(helpers.normalizeChatUi("glass"), "glass");
});

test("normalizeChatUi maps unknowns to minimal", () => {
  const { helpers } = setupTestEnvironment();
  assert.equal(helpers.normalizeChatUi("unknown"), "minimal");
  assert.equal(helpers.normalizeChatUi("malicious<script>"), "minimal");
  assert.equal(helpers.normalizeChatUi(""), "minimal");
  assert.equal(helpers.normalizeChatUi(null), "minimal");
  assert.equal(helpers.normalizeChatUi(undefined), "minimal");
});

test("applyChatUiClass removes all designs and adds the chosen one", () => {
  const { mockRoot, helpers } = setupTestEnvironment();

  // Start with minimal
  helpers.applyChatUiClass(mockRoot, "minimal");
  assert.ok(mockRoot.classList.contains("chat-ui-minimal"), "minimal should be applied");
  assert.ok(!mockRoot.classList.contains("chat-ui-terminal"), "terminal should not be present");
  assert.ok(!mockRoot.classList.contains("chat-ui-glass"), "glass should not be present");

  // Swap to terminal
  helpers.applyChatUiClass(mockRoot, "terminal");
  assert.ok(!mockRoot.classList.contains("chat-ui-minimal"), "minimal should be removed");
  assert.ok(mockRoot.classList.contains("chat-ui-terminal"), "terminal should be applied");
  assert.ok(!mockRoot.classList.contains("chat-ui-glass"), "glass should not be present");

  // Swap to glass
  helpers.applyChatUiClass(mockRoot, "glass");
  assert.ok(!mockRoot.classList.contains("chat-ui-minimal"), "minimal should not be present");
  assert.ok(!mockRoot.classList.contains("chat-ui-terminal"), "terminal should be removed");
  assert.ok(mockRoot.classList.contains("chat-ui-glass"), "glass should be applied");
});

test("applyChatUiClass normalizes unknown designs to minimal", () => {
  const { mockRoot, helpers } = setupTestEnvironment();

  // Set to terminal first
  helpers.applyChatUiClass(mockRoot, "terminal");
  assert.ok(mockRoot.classList.contains("chat-ui-terminal"), "terminal should be applied");

  // Try to apply an unknown design
  helpers.applyChatUiClass(mockRoot, "malicious<script>");
  assert.ok(!mockRoot.classList.contains("chat-ui-terminal"), "terminal should be removed");
  assert.ok(mockRoot.classList.contains("chat-ui-minimal"), "should fall back to minimal");
  assert.ok(!mockRoot.classList.contains("chat-ui-malicious<script>"), "malicious class should not be added");
});

test("applyChatUiClass handles cold-open scenario", () => {
  const { mockRoot, helpers } = setupTestEnvironment();

  // Cold-open with glass design (no prior classes)
  helpers.applyChatUiClass(mockRoot, "glass");
  assert.ok(mockRoot.classList.contains("chat-ui-glass"), "glass should be applied on cold-open");
  assert.equal(mockRoot.classList.length, 1, "only one class should be present");
});

test("applyChatUiClass handles empty/null design gracefully", () => {
  const { mockRoot, helpers } = setupTestEnvironment();

  helpers.applyChatUiClass(mockRoot, "terminal");
  assert.ok(mockRoot.classList.contains("chat-ui-terminal"), "terminal should be applied");

  // Apply null/empty design
  helpers.applyChatUiClass(mockRoot, null);
  assert.ok(!mockRoot.classList.contains("chat-ui-terminal"), "terminal should be removed");
  assert.ok(mockRoot.classList.contains("chat-ui-minimal"), "should fall back to minimal");
});
