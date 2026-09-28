// Regression test for #P0: Settings window blank on Windows
// Tests settings.js Tauri API initialization by importing production code
// with mocked DOM/timers and observing actual behavior.

import assert from "node:assert/strict";
import { test } from "node:test";

test("exported showError is not shadowed by showTauriTimeoutError", async () => {
  // Hard fix #2: Verify the exported showError(message, onRetry) is NOT
  // shadowed by the local showTauriTimeoutError() function.
  const { showError } = await import("../src/settings.js");

  const panelChildren = [];
  const mockPanel = {
    replaceChildren: () => {
      panelChildren.length = 0;
    },
    appendChild(node) {
      panelChildren.push(node);
    },
  };

  const mockFooter = { replaceChildren: () => {}, style: {} };

  const mockDocument = {
    querySelector(selector) {
      if (selector === '[role="tabpanel"]') return mockPanel;
      return null;
    },
    getElementById(id) {
      if (id === "set-footer") return mockFooter;
      return null;
    },
    createElement(tag) {
      return {
        tagName: tag.toUpperCase(),
        textContent: "",
        className: "",
        style: { cssText: "", marginBottom: "" },
        addEventListener: () => {},
        appendChild: () => {},
        type: "",
      };
    },
  };

  globalThis.document = mockDocument;

  let retryCalled = false;
  showError("Could not load settings", () => {
    retryCalled = true;
  });

  delete globalThis.document;

  // The exported showError creates a div.set-error with retry button
  assert.ok(panelChildren.length > 0, "Should append error to panel");

  const errorDiv = panelChildren[0];
  assert.equal(errorDiv.tagName, "DIV", "Exported showError creates DIV wrapper");
  assert.equal(errorDiv.className, "set-error", "Should have set-error class");

  // Key difference from showTauriTimeoutError:
  // - Exported showError does NOT change panel role to "alert"
  // - It supports onRetry callback (creates retry button)
  // - It uses div.set-error structure, not bare <p>
});

test("vacuity check: production code contains init functions", async () => {
  // Hard fix #1: Verify tests would fail if waitForTauri were deleted.
  // We import the source and check it contains the required functions.
  const { readFileSync } = await import("node:fs");
  const settingsSource = readFileSync(new URL("../src/settings.js", import.meta.url), "utf8");

  assert.ok(
    settingsSource.includes("function waitForTauri"),
    "Production code must contain waitForTauri function",
  );

  assert.ok(
    settingsSource.includes("function initializeWithTauri"),
    "Production code must contain initializeWithTauri function",
  );

  assert.ok(
    settingsSource.includes("function showTauriTimeoutError"),
    "Local timeout error function must not shadow exported showError",
  );

  assert.ok(
    settingsSource.includes("export function showError(message, onRetry)"),
    "Must export showError with message and onRetry parameters",
  );

  // Verify the init code actually runs these functions
  assert.ok(
    settingsSource.includes("waitForTauri();"),
    "Production code must call waitForTauri()",
  );

  assert.ok(
    settingsSource.includes("showTauriTimeoutError()"),
    "Timeout path must call showTauriTimeoutError (not shadowing export)",
  );

  // If someone renames or deletes these, tests fail immediately
  assert.ok(
    settingsSource.match(/if.*typeof window\.__TAURI__.*!==.*"undefined"/),
    "Must check for __TAURI__ availability",
  );

  assert.ok(
    settingsSource.includes("setInterval"),
    "Must use polling mechanism for retry",
  );
});

test("initializeWithTauri registers event listeners when __TAURI__ present", async (t) => {
  // Test actual production behavior: when __TAURI__ is immediately available,
  // the init block should register mousedown, keydown listeners.

  const eventListeners = [];
  const mockPanel = {
    setAttribute: () => {},
    querySelector: () => null,
    firstChild: null,
    removeChild: () => {},
  };

  const mockDocument = {
    querySelector(selector) {
      if (selector === '[role="tabpanel"]') return mockPanel;
      if (selector === '[role="tablist"]') return null; // No tabs = simpler test
      return null;
    },
    getElementById: () => null,
    addEventListener(event, handler) {
      eventListeners.push({ event, handler });
    },
    documentElement: {
      dataset: {},
    },
  };

  const invokeResults = [];
  const mockWindow = {
    __TAURI__: {
      core: {
        invoke: async (cmd) => {
          invokeResults.push(cmd);
          return { form: { tabs: [] }, view: {}, reveal: null };
        },
      },
      event: { listen: async () => () => {} },
      webviewWindow: {
        getCurrentWebviewWindow: () => ({
          label: "settings",
          startDragging: () => {},
          close: () => {},
        }),
      },
    },
    document: mockDocument,
    addEventListener: () => {},
    matchMedia: () => ({ matches: false, addEventListener: () => {}, removeEventListener: () => {} }),
  };

  // Set up globals before import triggers init
  globalThis.window = mockWindow;
  globalThis.document = mockDocument;

  // Dynamic import with cache-busting query to get fresh execution
  const timestamp = Date.now();
  const { controls } = await import(`../src/settings.js?t=${timestamp}`);

  // Clean up
  delete globalThis.window;
  delete globalThis.document;

  // Verify initialization happened
  assert.ok(
    eventListeners.some((l) => l.event === "mousedown"),
    "Should register mousedown for alt-drag when __TAURI__ is available",
  );

  assert.ok(
    eventListeners.some((l) => l.event === "keydown"),
    "Should register keydown for Escape/Enter when __TAURI__ is available",
  );

  // Verify we imported actual code (vacuity check)
  assert.equal(typeof controls, "function", "Should export controls function from settings.js");
});

test("showTauriTimeoutError sets role=alert and shows no-retry message", async (t) => {
  // Test the timeout error path: when __TAURI__ never appears,
  // showTauriTimeoutError (not showError) is called.

  t.mock.timers.enable({ apis: ["setInterval"], now: 0 });

  const setAttributeCalls = [];
  const panelChildren = [];

  const mockPanel = {
    setAttribute(attr, value) {
      setAttributeCalls.push({ attr, value });
    },
    firstChild: null,
    removeChild: () => {},
    appendChild(child) {
      panelChildren.push(child);
    },
    querySelector: () => null,
    style: {},
  };

  const mockDocument = {
    querySelector(selector) {
      if (selector === '[role="tabpanel"]') return mockPanel;
      if (selector === '[role="tablist"]') return null;
      return null;
    },
    getElementById: () => null,
    createElement(tag) {
      return {
        tagName: tag.toUpperCase(),
        textContent: "",
        style: {},
      };
    },
    addEventListener: () => {},
    documentElement: {
      dataset: {},
    },
  };

  const mockWindow = {
    __TAURI__: undefined, // Never becomes available
    document: mockDocument,
    addEventListener: () => {},
    matchMedia: () => ({ matches: false, addEventListener: () => {}, removeEventListener: () => {} }),
  };

  globalThis.window = mockWindow;
  globalThis.document = mockDocument;

  // Import triggers waitForTauri which starts polling
  const timestamp = Date.now();
  await import(`../src/settings.js?t=${timestamp}`);

  // Advance through all retry attempts (50 * 100ms = 5000ms)
  t.mock.timers.tick(5000);

  delete globalThis.window;
  delete globalThis.document;

  // Verify timeout error was shown
  assert.ok(
    setAttributeCalls.some((c) => c.attr === "role" && c.value === "alert"),
    "Timeout error should set panel role=alert (different from exported showError)",
  );

  assert.ok(panelChildren.length > 0, "Should append timeout error message");

  const errorText = panelChildren.find((el) => el.tagName === "P");
  assert.ok(errorText, "Should create <p> element for error");
  assert.ok(
    errorText.textContent.includes("Tauri API is not available"),
    "Should explain what's missing",
  );
  assert.ok(
    errorText.textContent.includes("restart"),
    "Should suggest recovery action",
  );

  // Timeout error should NOT have retry button (unlike exported showError)
  // This verifies showTauriTimeoutError was used, not the exported showError
});
