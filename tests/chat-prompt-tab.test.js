import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { promptTabFromOpening } from "../src/chat-prompt-tab.js";

const html = readFileSync(new URL("../src/chat.html", import.meta.url), "utf8");
const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");

test("an opening that never asked cannot draw the authored layout", () => {
  const view = promptTabFromOpening({
    personality: "Nim is patient.",
    instance_prompt: "Answer in haiku.",
  });

  assert.equal(
    view.authored,
    false,
    "Personality Prompt and Instance Prompt are not in force until Blank AI is asked",
  );
});

test("Blank AI on is the moment only", () => {
  const view = promptTabFromOpening({
    blank: true,
    personality: "Nim is patient.",
    instance_prompt: "Answer in haiku.",
  });

  assert.equal(view.authored, false);
});

test("Blank AI off keeps both authored layers in force", () => {
  const view = promptTabFromOpening({
    blank: false,
    personality: "Nim is patient.",
    instance_prompt: "Answer in haiku.",
  });

  assert.equal(view.authored, true);
});

test("the Prompt tab markup can hide the authored layers", () => {
  assert.match(html, /id="prompt-blank"/);
  assert.match(html, /id="prompt-personality"/);
  assert.match(html, /The Character Prompt is not sent/);
});

test("chat.js asks the helper before it fills the authored layers", () => {
  assert.match(js, /import \{ promptTabFromOpening \} from "\.\/chat-prompt-tab\.js"/);
  const asked = js.indexOf("promptTabFromOpening(opening)");
  const filled = js.indexOf('getElementById("personality")');
  assert.ok(asked > 0, "showPrompt has to ask before it can draw");
  assert.ok(asked < filled, "an opening that never asked must not reach the Personality block");
});
