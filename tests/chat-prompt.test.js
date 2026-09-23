// ADR-0012's seam on this side of the wire: saving an Instance Prompt throws
// the Director session away, so the trigger has to be an explicit act. A
// keystroke listener would wipe the conversation mid-sentence, quietly.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");
const html = readFileSync(new URL("../src/chat.html", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");

function ruleBlock(source, selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`(?:^|\\n)\\s*${escaped}\\s*\\{([^}]+)\\}`));
  assert.ok(match, `${selector} has no rule of its own`);
  return match[1];
}

function promptSection(source) {
  const match = source.match(/<section class="prompt"[^>]*>([\s\S]*?)<\/section>/);
  assert.ok(match, "the Prompt tab section is missing");
  return match[1];
}

// Every event this window listens for, as `target.event`, in the order it
// wires them.
const LISTENED = [...js.matchAll(/(\w+)\.addEventListener\(\s*"([a-z-]+)"/g)].map(
  ([, target, name]) => `${target}.${name}`,
);

test("nothing in the Chat surface saves on a keystroke", () => {
  assert.ok(LISTENED.length >= 4, "the listener parse stopped matching — this cannot pass vacuously");

  const typing = LISTENED.filter((wiring) =>
    ["input", "keyup", "keydown", "keypress", "change"].includes(wiring.split(".")[1]),
  );

  // The composer's send key is the one listener that may fire mid-typing: the
  // field is a textarea, which does not submit its form on Enter,
  // so Enter had to be wired by hand. It sends a turn; it does not save.
  // Prompt-tab input/change only arm Save and Discard; they must not save.
  assert.deepEqual(
    [...typing].sort(),
    ["line.keydown", "promptText.change", "promptText.input"].sort(),
    `these fire while the user is still typing, and saving reopens the session: ${typing.join(", ")}`,
  );

  const at = js.indexOf('line.addEventListener("keydown"');
  const end = js.indexOf("\n});", at);
  assert.doesNotMatch(
    js.slice(at, end),
    /invoke\(/,
    "the send key reaches the Shell only through the submit listener, never the save",
  );

  for (const name of ["input", "change"]) {
    const start = js.indexOf(`promptText.addEventListener("${name}"`);
    const stop = js.indexOf(";", start);
    const body = js.slice(start, stop);
    assert.doesNotMatch(body, /invoke\(/, `promptText.${name} must not reach the Shell`);
    assert.doesNotMatch(
      body,
      /askingToSave\(true\)/,
      `promptText.${name} must not open the confirm`,
    );
  }
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
  assert.ok(save > 0, "and the button that spends it is on the tab");
});

test("Prompt tab actions sit after the authored layers, outside the scroll", () => {
  const section = promptSection(html);
  const prompt = ruleBlock(css, ".prompt");
  const actions = ruleBlock(css, ".prompt-actions");
  const body = ruleBlock(css, ".prompt-body");
  const composer = ruleBlock(css, ".composer");

  assert.match(
    section,
    /^\s*<div class="prompt-body">[\s\S]*?<div class="prompt-actions">/,
    "Save sits after the layers, a footer under the scrolling body",
  );
  assert.match(prompt, /display:\s*flex/, "the tab is a column so the footer need not scroll");
  assert.match(prompt, /flex-direction:\s*column/);
  assert.match(prompt, /overflow:\s*hidden/, "the pane does not scroll; .prompt-body does");
  assert.match(prompt, /min-height:\s*0/, "the flex child can shrink below its content");
  assert.match(actions, /flex:\s*0 0 auto/, "the footer keeps its height");
  assert.match(actions, /border-top:/, "the footer is a composer twin, not a toolbar");
  assert.doesNotMatch(actions, /border-bottom:/);
  assert.match(
    actions,
    /padding:\s*12px 16px 16px/,
    "padding matches Chat's composer, not a top toolbar",
  );
  assert.match(composer, /padding:\s*12px 16px 16px/, "the composer padding the footer twins");
  assert.match(body, /flex:\s*1 1 auto/, "the layers take the leftover height");
  assert.match(body, /min-height:\s*0/, "so the flex child can shrink and scroll");
  assert.match(body, /overflow-y:\s*auto/, "Personality and the field scroll above the footer");
  assert.match(body, /display:\s*grid/, "the authored layers still lay out as a grid");
  assert.match(
    body,
    /grid-auto-rows:\s*max-content/,
    "long app-level copy sizes its own row instead of overflowing the next heading",
  );
});

test("the tab shows the three concatenated layers, app then Character then instance", () => {
  const section = promptSection(html);
  const app = section.indexOf('id="instructions"');
  const character = section.indexOf('id="personality"');
  const instance = section.indexOf('id="prompt-text"');

  assert.ok(app > 0, "app-level instructions, frozen");
  assert.ok(character > app, "then the Character's Personality Prompt");
  assert.ok(instance > character, "then the user's Instance Prompt");
  assert.match(
    section,
    /concatenat/i,
    "the copy says the three layers are concatenated",
  );
  assert.match(
    section,
    /new session/i,
    "and that editing the instance layer starts a new session",
  );
  assert.doesNotMatch(
    html,
    /what just happened:/,
    "this moment is per-wake, not a frozen layer on the tab",
  );
});

test("every empty Prompt tab layer says Empty", () => {
  assert.match(
    html,
    /id="prompt-text"[\s\S]*?placeholder="Empty/,
    "the Instance Prompt field names Empty when nobody wrote one",
  );
  assert.match(js, /fillFrozen\(/, "frozen layers share one empty seat");
  assert.match(js, /"Empty"/, "that seat reads Empty, not a collapsed box");
  const empty = ruleBlock(css, ".frozen.is-empty");
  assert.match(
    empty,
    /color:\s*var\(--chat-ink-4\)/,
    "Empty on a frozen layer matches the textarea placeholder",
  );
});

test("an empty Instance Prompt field keeps rows and matches the Personality block", () => {
  const field = ruleBlock(css, ".prompt-field");
  const textarea = ruleBlock(css, ".prompt textarea");
  const frozen = ruleBlock(css, ".frozen");
  const threeLines = /min-height:\s*calc\(\s*1\.5em\s*\*\s*3\s*\+\s*20px\s*\)/;

  assert.match(field, /width:\s*100%/, "the wrapper is as wide as the frozen Personality block");
  assert.match(field, /min-width:\s*0/, "so the grid item can shrink to that column");
  assert.match(textarea, /display:\s*block/, "block, so the wrapper's height is the field's border box");
  assert.match(textarea, /width:\s*100%/, "as wide as the wrapper, which matches the Personality block");
  assert.match(textarea, /min-width:\s*0/, "so the field can shrink with the wrapper");
  assert.match(textarea, /box-sizing:\s*border-box/, "border sits inside the width the wrapper gives it");
  assert.match(textarea, threeLines, "empty field cannot collapse below about three lines of copy");
  assert.match(frozen, threeLines, "empty Personality keeps the same three-line floor");
  assert.match(frozen, /box-sizing:\s*border-box/, "so that floor includes padding, as the field's does");
  assert.doesNotMatch(
    textarea,
    /align-self:/,
    "the textarea is not a grid item; stretch on it is what collapsed the empty field",
  );
});

test("the save warning sits outside the Instance Prompt field", () => {
  const section = promptSection(html);
  const body = ruleBlock(css, ".prompt-body");

  assert.match(body, /display:\s*grid/, "the authored layers still lay out as a grid");
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
    "a textarea that is a direct grid child is a replaced item and paints over .warn",
  );
  assert.match(
    section,
    /<\/div>\s*<p class="warn">/,
    "the warning follows the wrapper as a later grid sibling",
  );
});

function listenerSlice(source, target, event) {
  const token = `${target}.addEventListener("${event}"`;
  const start = source.indexOf(token);
  assert.ok(start >= 0, `${target}.${event} is missing`);
  const next = source.indexOf(".addEventListener(", start + token.length);
  return source.slice(start, next < 0 ? undefined : next);
}

test("Prompt tab Save and Discard stay disabled until the field is dirty", () => {
  const section = promptSection(html);

  assert.match(section, /id="prompt-discard"/, "Discard sits in the sticky footer");
  assert.match(
    section,
    /id="prompt-save"[^>]*\bdisabled\b/,
    "Save starts disabled: a clean field must not open the confirm",
  );
  assert.match(
    section,
    /id="prompt-discard"[^>]*\bdisabled\b/,
    "Discard starts disabled with Save",
  );

  const discard = ruleBlock(css, "#prompt-discard");
  assert.match(discard, /background:\s*transparent/, "Discard is the outline twin of Cancel, not accent");
  assert.match(ruleBlock(css, "#prompt-save:disabled"), /cursor:\s*default|background:\s*var\(--chat-fill-soft\)/);

  assert.match(js, /function promptDirty\(/);
  assert.match(js, /promptText\.value !== savedPrompt/);
  assert.match(
    js,
    /promptSave\.disabled = confirming \|\| !dirty/,
    "Save is disabled while clean or confirming",
  );
  assert.match(
    js,
    /promptDiscard\.disabled = confirming \|\| !dirty/,
    "Discard is disabled while clean or confirming",
  );
  assert.match(js, /promptDiscard\.hidden = asking/, "Discard does not compete with Confirm/Cancel");
  assert.match(js, /showPrompt\([\s\S]*?syncPromptActions\(\)/s);

  const saveClick = listenerSlice(js, "promptSave", "click");
  assert.match(saveClick, /if \(!promptDirty\(\)\)/, "a clean Save click is a no-op, not a confirm");
  assert.match(saveClick, /askingToSave\(true\)/);
});

test("Discard restores the last saved text; Cancel only aborts confirm", () => {
  const discard = listenerSlice(js, "promptDiscard", "click");
  assert.match(discard, /promptText\.value = savedPrompt/, "Discard puts the last saved text back");
  assert.match(discard, /promptSaid\.textContent = ""/);
  assert.match(discard, /askingToSave\(false\)/, "and leaves confirm if it was open");
  assert.doesNotMatch(discard, /invoke\(/, "Discard never reaches the Shell");

  const cancel = listenerSlice(js, "promptCancel", "click");
  assert.match(cancel, /askingToSave\(false\)/);
  assert.doesNotMatch(
    cancel,
    /promptText\.value/,
    "Cancel aborts confirm only; the field keeps the unsaved sentence",
  );
});
