import assert from "node:assert/strict";
import { test } from "node:test";

import { composerPlaceholder } from "../src/chat-connect.js";

test("when nothing can answer, the composer keeps the disabled string", () => {
  assert.equal(
    composerPlaceholder({ name: "bmo", configured: false, enabled: false }),
    "Nothing can answer yet",
  );
  assert.equal(
    composerPlaceholder({ name: "bmo", configured: true, enabled: false }),
    "Nothing can answer yet",
  );
  assert.equal(
    composerPlaceholder({
      name: "bmo",
      configured: true,
      enabled: true,
      login: "claude /login",
    }),
    "Nothing can answer yet",
  );
});

test("a Harness that is set but not running is not something that can answer", () => {
  assert.equal(
    composerPlaceholder({
      name: "bmo",
      configured: true,
      enabled: true,
      harness: { name: "hermes", session: null, alive: false, login: null },
    }),
    "Nothing can answer yet",
  );
});

// DROPPED (#1268): "the composer markup does not ship Say something as the live placeholder"
// asserts the html placeholder attribute is not that string.
// DROPPED (#1268): "unanswerable empty-state copy is unchanged"
// matches Nothing to answer with in chat.html.
// DROPPED (#1268): "chat.js asks the helper for the composer placeholder"
// counts composerPlaceholder( occurrences in chat.js.
