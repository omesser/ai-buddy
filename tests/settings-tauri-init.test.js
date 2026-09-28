// Regression test for #P0: Settings window blank on Windows
// Tests settings.js Tauri API initialization by importing production code
// with mocked DOM/timers and observing actual behavior.

import assert from "node:assert/strict";
import { test } from "node:test";

test("exported showError creates retry button that invokes onRetry callback", async () => {
  // Soft #3: Test that retry button is created and clicking it invokes onRetry
  const { showError } = await import("../src/settings.js");

  const panelChildren = [];
  const buttonChildren = [];

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
      const element = {
        tagName: tag.toUpperCase(),
        textContent: "",
        className: "",
        style: { cssText: "", marginBottom: "" },
        type: "",
        children: buttonChildren,
        appendChild(child) {
          buttonChildren.push(child);
        },
        addEventListener(event, handler) {
          // Store the click handler so we can invoke it
          if (event === "click") {
            element._clickHandler = handler;
          }
        },
      };
      return element;
    },
  };

  globalThis.document = mockDocument;

  let retryCalled = false;
  showError("Could not load settings", () => {
    retryCalled = true;
  });

  delete globalThis.document;

  // Verify exported showError creates proper structure with retry
  assert.ok(panelChildren.length > 0, "Should append error to panel");
  const errorDiv = panelChildren[0];
  assert.equal(errorDiv.tagName, "DIV", "Exported showError creates DIV wrapper");
  assert.equal(errorDiv.className, "set-error", "Should have set-error class");

  // Verify retry button was created
  const retryButton = buttonChildren.find((el) => el.tagName === "BUTTON");
  assert.ok(retryButton, "Should create a retry button");
  assert.equal(retryButton.textContent, "Retry", "Button should say 'Retry'");
  assert.equal(retryButton.type, "button", "Button type should be 'button'");

  // Click the retry button and verify callback is invoked
  assert.equal(retryCalled, false, "Retry not called yet");
  if (retryButton._clickHandler) {
    retryButton._clickHandler();
  }
  assert.equal(retryCalled, true, "Clicking retry button should invoke onRetry callback");

  // Key difference from showTauriTimeoutError:
  // - Exported showError does NOT change panel role to "alert"
  // - It supports onRetry callback (creates retry button)
  // - It uses div.set-error structure, not bare <p>
});

test("__TAURI__ appears mid-poll after some failed attempts (Windows race)", async (t) => {
  // Soft #2: Test the Windows race condition where __TAURI__ appears after
  // several polling attempts have already failed.
  t.mock.timers.enable({ apis: ["setInterval"], now: 0 });

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
      if (selector === '[role="tablist"]') return null;
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

  const mockWindow = {
    __TAURI__: undefined, // Initially undefined
    document: mockDocument,
    addEventListener: () => {},
    matchMedia: () => ({ matches: false, addEventListener: () => {}, removeEventListener: () => {} }),
  };

  globalThis.window = mockWindow;
  globalThis.document = mockDocument;

  // Import triggers waitForTauri which starts polling
  const timestamp = Date.now();
  await import(`../src/settings.js?t=${timestamp}`);

  // Initially no initialization
  assert.equal(eventListeners.length, 0, "Should not initialize when __TAURI__ is missing");

  // Advance through 10 failed poll attempts (1000ms)
  t.mock.timers.tick(1000);
  assert.equal(eventListeners.length, 0, "Should still not initialize after failed polls");

  // Now make __TAURI__ available (simulating Windows WebView2 delayed injection)
  mockWindow.__TAURI__ = {
    core: { invoke: async () => ({ form: { tabs: [] }, view: {}, reveal: null }) },
    event: { listen: async () => () => {} },
    webviewWindow: {
      getCurrentWebviewWindow: () => ({
        label: "settings",
        startDragging: () => {},
        close: () => {},
      }),
    },
  };

  // Advance to next poll interval (100ms) - should detect __TAURI__ and initialize
  t.mock.timers.tick(100);

  delete globalThis.window;
  delete globalThis.document;

  // Verify initialization happened after __TAURI__ became available mid-poll
  assert.ok(
    eventListeners.length > 0,
    "Should initialize after __TAURI__ appears mid-poll (Windows race condition)",
  );
  assert.ok(
    eventListeners.some((l) => l.event === "mousedown"),
    "Should register mousedown listener after delayed init",
  );
  assert.ok(
    eventListeners.some((l) => l.event === "keydown"),
    "Should register keydown listeners after delayed init",
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
