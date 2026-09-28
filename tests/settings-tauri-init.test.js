// Regression test for #P0: Settings window blank on Windows
// When window.__TAURI__ is undefined at script load, settings.js should retry
// or show an error instead of silently failing (blank white screen).

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const settingsJs = readFileSync(new URL("../src/settings.js", import.meta.url), "utf8");

test("settings.js handles missing __TAURI__ gracefully, not silently", () => {
  // The bug: when __TAURI__ is undefined, the entire initialization block
  // (lines ~639-692) is skipped, leaving a blank white page.
  
  // settings.js should either:
  // 1. Retry/poll for __TAURI__ to become available, OR
  // 2. Show an error message to the user
  
  // This test verifies the code doesn't silently fail when __TAURI__ is undefined.
  // The fix should add either a retry loop or error display.
  
  const hasConditionalInit = settingsJs.includes('if (typeof window.__TAURI__ !== "undefined")');
  assert.ok(
    hasConditionalInit,
    "Current code conditionally initializes only if __TAURI__ is defined",
  );
  
  // After fix, the code should have ONE of these:
  // - A retry/polling mechanism (e.g., setInterval, setTimeout with retry)
  // - An error message displayed when __TAURI__ is never available
  // - A waitForTauri or similar helper that polls for the API
  
  const hasRetryMechanism =
    settingsJs.includes("setInterval") ||
    settingsJs.includes("waitFor") ||
    settingsJs.match(/setTimeout.*__TAURI__/);
  
  const hasErrorDisplay =
    settingsJs.match(/textContent.*__TAURI__|Tauri.*not.*available/i) ||
    settingsJs.includes('role="alert"');
  
  // This test will fail until the fix is implemented
  const hasGracefulHandling = hasRetryMechanism || hasErrorDisplay;
  
  assert.ok(
    hasGracefulHandling,
    "settings.js should retry for __TAURI__ or show error, not silently fail with blank screen. " +
      "Found retry=" + Boolean(hasRetryMechanism) + " errorDisplay=" + Boolean(hasErrorDisplay),
  );
});

test("settings.js has waitForTauri that handles delayed API injection", () => {
  // After the fix, there should be a waitForTauri function that:
  // 1. Checks if __TAURI__ is available immediately
  // 2. Polls for it if not available
  // 3. Shows an error after timeout
  
  const hasWaitForTauri = settingsJs.includes("waitForTauri");
  assert.ok(
    hasWaitForTauri,
    "settings.js should have a waitForTauri function to handle delayed API injection",
  );
  
  const callsWaitForTauri = settingsJs.includes("waitForTauri();");
  assert.ok(
    callsWaitForTauri,
    "settings.js should call waitForTauri() to start initialization",
  );
  
  const hasInitializeWithTauri = settingsJs.includes("initializeWithTauri");
  assert.ok(
    hasInitializeWithTauri,
    "settings.js should have initializeWithTauri function called once __TAURI__ is available",
  );
});
