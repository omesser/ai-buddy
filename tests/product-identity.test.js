import assert from "node:assert/strict";
import { test } from "node:test";

import { createNamesNotice } from "../src/chat-names-hint.js";

test("the names notice calls the on-screen presence a fidget", () => {
  const notice = createNamesNotice({ act: async () => "quiet" });
  const { view } = notice.receive({ hint: "due", generation: 1 });
  assert.equal(
    view.body,
    "The fidget knows where your windows are, not what they are. One switch in Settings turns on titles and application names together.",
  );
});
