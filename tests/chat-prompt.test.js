// Run with `node --test tests/`.
//
// ADR-0012's seam on this side of the wire: saving an Instance Prompt throws
// the Director session away, so the trigger has to be an explicit act. A
// keystroke listener would wipe the conversation mid-sentence, and it would do
// it quietly — the window still works, and the user only finds out afterwards.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");
const html = readFileSync(new URL("../src/chat.html", import.meta.url), "utf8");

// Every event this window listens for, in the order it wires them.
const LISTENED = [...js.matchAll(/addEventListener\(\s*"([a-z-]+)"/g)].map(([, name]) => name);

test("nothing in the Chat surface saves on a keystroke", () => {
  const typing = LISTENED.filter((name) =>
    ["input", "keyup", "keydown", "keypress", "change"].includes(name),
  );

  assert.deepEqual(
    typing,
    [],
    `these fire while the user is still typing, and saving reopens the session: ${typing.join(", ")}`,
  );
});

test("the save is asked for twice before the conversation is spent", () => {
  assert.match(js, /promptSave\.addEventListener\("click"/);
  assert.match(js, /promptConfirm\.addEventListener\("click"/);

  // The first click only asks; the call itself hangs off the second.
  const asked = js.indexOf('promptConfirm.addEventListener("click"');
  const called = js.indexOf('invoke("chat_prompt"');
  assert.ok(called > asked, "chat_prompt must be invoked from the confirming click");
  assert.equal(
    js.split('invoke("chat_prompt"').length - 1,
    1,
    "one save path, so one place the warning has to be shown before",
  );
});

test("the tab warns that saving starts a new conversation before it offers to", () => {
  const warning = html.indexOf("Saving starts a new conversation");
  const save = html.indexOf('id="prompt-save"');

  assert.ok(warning > 0, "the cost of saving is written on the tab");
  assert.ok(save > warning, "and it is written above the button that spends it");
});

test("the tab shows the two authored layers and not the assembled payload", () => {
  assert.match(html, /id="personality"/, "the Character's own layer, frozen");
  assert.match(html, /id="prompt-text"/, "and the user's, editable");
  assert.doesNotMatch(
    html,
    /You may propose one of these behaviors/,
    "the assembled Character Prompt stays inspectable in settings, not here",
  );
});
