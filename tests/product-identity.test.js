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

// DROPPED (#1268): "window titles use the product name Fidget"
// matches <title>Fidget in html files.
// DROPPED (#1268): "the bundle identifier and product name are Fidget"
// asserts tauri.conf productName and identifier strings.
// DROPPED (#1268): "development switches are FIDGET_ variables"
// matches Flag::new("FIDGET_TRACE_FRAMES") in source.
