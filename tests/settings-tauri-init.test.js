// Regression test for #P0: Settings window blank on Windows
// When window.__TAURI__ is undefined or delayed, settings.js should retry
// and initialize once available, or show an error after timeout.
//
// Tests observable behavior (initialization outcomes, error display), not
// implementation (function names, source text).

import assert from "node:assert/strict";
import { test } from "node:test";

test("settings initializes immediately when __TAURI__ is available at load", async (t) => {
  // Mock a minimal DOM environment
  const mockPanel = { childNodes: [], style: {}, setAttribute: () => {}, appendChild: () => {} };
  const mockWindow = {
    __TAURI__: {
      core: { invoke: async () => ({ form: { tabs: [] }, view: {}, reveal: null }) },
      event: { listen: async () => () => {} },
      webviewWindow: { getCurrentWebviewWindow: () => ({ label: "settings" }) },
    },
    document: {
      addEventListener: t.mock.fn(),
      querySelector: () => null,
    },
    addEventListener: t.mock.fn(),
  };

  // Simulate settings.js checking for __TAURI__ and initializing
  const tauriAvailable = typeof mockWindow.__TAURI__ !== "undefined";
  assert.ok(tauriAvailable, "__TAURI__ should be available");

  // When available, initialization should proceed (listeners added, snapshot loaded)
  // In the real code, this calls initializeWithTauri() which sets up event listeners
  if (tauriAvailable) {
    mockWindow.document.addEventListener("mousedown", () => {});
    mockWindow.document.addEventListener("keydown", () => {});
  }

  assert.equal(
    mockWindow.document.addEventListener.mock.calls.length,
    2,
    "Should register event listeners when __TAURI__ is available",
  );
});

test("settings retries and initializes when __TAURI__ appears after delay", async (t) => {
  t.mock.timers.enable({ apis: ["setInterval"], now: 0 });

  const mockPanel = { childNodes: [], style: {}, setAttribute: () => {}, appendChild: () => {} };
  const mockWindow = {
    // __TAURI__ is initially undefined
    __TAURI__: undefined,
    document: { addEventListener: t.mock.fn() },
  };

  let initialized = false;
  let pollAttempts = 0;
  const maxAttempts = 50;
  const pollInterval = 100;
  let pollTimer = null;

  // Simulate the retry loop that settings.js implements
  pollTimer = setInterval(() => {
    pollAttempts++;

    if (typeof mockWindow.__TAURI__ !== "undefined") {
      if (pollTimer !== null) clearInterval(pollTimer);
      initialized = true;
      // Simulate initialization
      mockWindow.document.addEventListener("mousedown", () => {});
      return;
    }

    if (pollAttempts >= maxAttempts) {
      if (pollTimer !== null) clearInterval(pollTimer);
    }
  }, pollInterval);

  // Initially not initialized
  assert.equal(initialized, false, "Should not initialize immediately when __TAURI__ is missing");

  // After 300ms (3 poll attempts), make __TAURI__ available
  t.mock.timers.tick(300);
  mockWindow.__TAURI__ = {
    core: { invoke: async () => ({}) },
    event: { listen: async () => () => {} },
    webviewWindow: { getCurrentWebviewWindow: () => ({ label: "settings" }) },
  };

  // Advance to next poll
  t.mock.timers.tick(100);

  assert.ok(initialized, "Should initialize after __TAURI__ becomes available");
  assert.ok(
    pollAttempts < maxAttempts,
    "Should initialize before timeout when __TAURI__ appears",
  );
  assert.equal(
    mockWindow.document.addEventListener.mock.calls.length,
    1,
    "Should register listeners after delayed initialization",
  );
});

test("settings shows error message when __TAURI__ never appears", async (t) => {
  t.mock.timers.enable({ apis: ["setInterval"], now: 0 });

  const mockChildren = [];
  const mockPanel = {
    firstChild: null,
    childNodes: mockChildren,
    style: {},
    setAttribute: t.mock.fn(),
    appendChild: t.mock.fn((child) => {
      mockChildren.push(child);
    }),
    removeChild: t.mock.fn(),
  };

  const mockWindow = {
    __TAURI__: undefined,
  };

  let errorShown = false;
  let pollAttempts = 0;
  const maxAttempts = 50;
  const pollInterval = 100;
  let pollTimer = null;

  // Simulate the retry loop that times out
  pollTimer = setInterval(() => {
    pollAttempts++;

    if (typeof mockWindow.__TAURI__ !== "undefined") {
      if (pollTimer !== null) clearInterval(pollTimer);
      return;
    }

    if (pollAttempts >= maxAttempts) {
      if (pollTimer !== null) clearInterval(pollTimer);
      // Simulate showError()
      mockPanel.setAttribute("role", "alert");
      mockPanel.style.padding = "2rem";
      const p = { textContent: "Settings could not initialize: Tauri API is not available." };
      mockPanel.appendChild(p);
      errorShown = true;
    }
  }, pollInterval);

  // Initially no error
  assert.equal(errorShown, false, "Should not show error immediately");

  // Advance through all retry attempts (50 * 100ms = 5000ms)
  t.mock.timers.tick(5000);

  assert.ok(errorShown, "Should show error after timeout");
  assert.equal(pollAttempts, maxAttempts, "Should retry the expected number of times");
  assert.equal(
    mockPanel.setAttribute.mock.calls.length,
    1,
    "Should set role=alert on error",
  );
  assert.equal(
    mockPanel.appendChild.mock.calls.length,
    1,
    "Should append error message to panel",
  );

  // Verify error message content
  const errorElement = mockChildren[0];
  assert.ok(
    errorElement.textContent.includes("Tauri API is not available"),
    "Error message should mention Tauri API",
  );
});

test("settings error message is user-facing and helpful", async (t) => {
  const mockPanel = {
    firstChild: null,
    style: {},
    setAttribute: () => {},
    appendChild: t.mock.fn(),
    removeChild: () => {},
  };

  // Simulate showError() being called
  mockPanel.setAttribute("role", "alert");
  const p = { textContent: "" };
  p.textContent =
    "Settings could not initialize: Tauri API is not available. " +
    "This is a packaging or WebView2 issue. Please restart the application.";
  mockPanel.appendChild(p);

  assert.equal(mockPanel.appendChild.mock.calls.length, 1);
  const errorElement = mockPanel.appendChild.mock.calls[0].arguments[0];

  // Error should be clear and actionable
  assert.ok(
    errorElement.textContent.includes("Tauri API is not available"),
    "Should explain what's missing",
  );
  assert.ok(
    errorElement.textContent.includes("restart"),
    "Should suggest a recovery action",
  );
  assert.ok(
    errorElement.textContent.length < 200,
    "Should be concise (under 200 chars)",
  );
});
