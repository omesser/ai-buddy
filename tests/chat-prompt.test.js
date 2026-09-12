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
const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");

function ruleBlock(source, selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`));
  assert.ok(match, `${selector} has no rule of its own`);
  return match[1];
}

function promptSection(source) {
  const match = source.match(/<section class="prompt"[^>]*>([\s\S]*?)<\/section>/);
  assert.ok(match, "the Prompt tab section is missing");
  return match[1];
}

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

test("an empty Instance Prompt field keeps rows and matches the Personality block", () => {
  const field = ruleBlock(css, ".prompt-field");
  const textarea = ruleBlock(css, ".prompt textarea");

  assert.match(field, /width:\s*100%/, "the wrapper is as wide as the frozen Personality block");
  assert.match(field, /min-width:\s*0/, "so the grid item can shrink to that column");
  assert.match(textarea, /display:\s*block/, "block, so the wrapper's height is the field's border box");
  assert.match(textarea, /width:\s*100%/, "as wide as the wrapper, which matches the Personality block");
  assert.match(textarea, /min-width:\s*0/, "so the field can shrink with the wrapper");
  assert.match(textarea, /box-sizing:\s*border-box/, "border sits inside the width the wrapper gives it");
  assert.match(
    textarea,
    /min-height:\s*calc\(\s*1\.5em\s*\*\s*3\s*\+\s*20px\s*\)/,
    "empty field cannot collapse below about three lines of copy",
  );
  assert.doesNotMatch(
    textarea,
    /align-self:/,
    "the textarea is not a grid item; stretch on it is what collapsed the empty field",
  );
});

test("the save warning sits outside the Instance Prompt field", () => {
  const section = promptSection(html);
  const prompt = ruleBlock(css, ".prompt");

  assert.match(prompt, /display:\s*grid/, "the Prompt tab still lays out as a grid");
  assert.match(
    section,
    /<div class="prompt-field">\s*<textarea\b[\s\S]*?id="prompt-text"[\s\S]*?<\/textarea>\s*<\/div>/,
    "the textarea is the wrapper's only child, so field chrome cannot cover .warn",
  );

  const wrapped = section.match(/<div class="prompt-field">([\s\S]*?)<\/div>/);
  assert.ok(wrapped, "the Instance Prompt field has a wrapper");
  assert.doesNotMatch(wrapped[1], /class="warn"/, "the warning is not inside the field wrapper");
  assert.doesNotMatch(wrapped[1], /<p[\s>]/, "the wrapper holds the textarea only");

  const rest = section.replace(/<div class="prompt-field">[\s\S]*?<\/div>/, "");
  assert.doesNotMatch(
    rest,
    /<textarea\b/,
    "a textarea that is a direct .prompt child is a replaced grid item and paints over .warn",
  );
  assert.match(
    section,
    /<\/div>\s*<p class="warn">/,
    "the warning follows the wrapper as a later grid sibling",
  );
});
