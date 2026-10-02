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
