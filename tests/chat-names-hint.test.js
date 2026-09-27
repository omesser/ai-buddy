import assert from "node:assert/strict";
import { test } from "node:test";

import { createNamesNotice } from "../src/chat-names-hint.js";

const BODY =
  "The fidget knows where your windows are, not what they are. One switch in Settings turns on titles and application names together.";

test("due shows the body and both buttons", () => {
  const notice = createNamesNotice({ act: async () => "quiet" });
  const { view } = notice.receive({ hint: "due", generation: 1 });

  assert.equal(view.visible, true);
  assert.equal(view.heading, "Window names are off");
  assert.equal(view.body, BODY);
  assert.deepEqual(view.buttons, [
    { action: "open-settings", label: "Open Settings" },
    { action: "dismiss", label: "Don't show this again" },
  ]);
});

test("quiet and dismissed hide the notice", () => {
  const quiet = createNamesNotice({ act: async () => "quiet" });
  const dismissed = createNamesNotice({ act: async () => "dismissed" });

  assert.equal(quiet.receive({ hint: "quiet", generation: 1 }).view.visible, false);
  assert.equal(dismissed.receive({ hint: "dismissed", generation: 1 }).view.visible, false);
});

test("opening settings twice still accepts a later quiet", async () => {
  const notice = createNamesNotice({
    act: async () => ({ hint: "due", generation: 1 }),
  });
  notice.receive({ hint: "due", generation: 1 });
  await notice.press("open-settings");
  await notice.press("open-settings");

  const quiet = notice.receive({ hint: "quiet", generation: 2 });

  assert.equal(quiet.changed, true);
  assert.equal(quiet.view.visible, false);
});

test("a lower generation does not replace a newer one", () => {
  const notice = createNamesNotice({ act: async () => "quiet" });
  notice.receive({ hint: "due", generation: 2 });

  const again = notice.receive({ hint: "quiet", generation: 1 });

  assert.equal(again.changed, false);
  assert.equal(again.view.visible, true);
  assert.equal(again.view.body, BODY);
});

test("the notice does not name a screen capture", () => {
  const notice = createNamesNotice({ act: async () => "quiet" });
  const { view } = notice.receive({ hint: "due", generation: 1 });
  const text = [view.heading, view.body, ...view.buttons.map((button) => button.label)].join("\n");

  for (const word of ["screenshot", "Capture", "Screen Recording", "ScreenCast"]) {
    assert.equal(text.includes(word), false, word);
  }
});
