// The composer is the only way a user gets text into a turn. As an `<input
// type="text">`, HTML's value sanitization deleted every LF and CR from a paste
// with nothing on screen saying so. The element is the bug, so the element is pinned.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const html = readFileSync(new URL("../src/chat.html", import.meta.url), "utf8");
const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");

function composerForm() {
  const match = html.match(/<form class="composer"[^>]*>([\s\S]*?)<\/form>/);
  assert.ok(match, "the composer form is missing");
  return match[1];
}

test("the composer holds a value that can carry newlines", () => {
  const form = composerForm();

  assert.match(
    form,
    /<textarea\b[^>]*\bid="line"/,
    "a `type=text` input strips LF and CR out of its value; only a textarea keeps them",
  );
  assert.doesNotMatch(
    form,
    /<input\b[^>]*\bid="line"/,
    "the sanitizing input is gone, not merely shadowed by a textarea beside it",
  );
});

// An `<input>` in a form submits on Enter for free; a textarea does not, so the
// send key is something this window has to say out loud.
function keydownHandler() {
  const at = js.indexOf('line.addEventListener("keydown"');
  assert.notEqual(at, -1, "the composer field has no keydown listener, so Enter no longer sends");
  const end = js.indexOf("\n});", at);
  assert.notEqual(end, -1, "the keydown listener is not closed the way this parse expects");
  return js.slice(at, end);
}

test("Enter sends and Shift+Enter keeps the newline the textarea can now hold", () => {
  const handler = keydownHandler();

  assert.match(handler, /event\.key\s*[!=]==\s*"Enter"/, "Enter is the send key");
  assert.match(
    handler,
    /event\.shiftKey/,
    "Shift+Enter has to fall through to the textarea; it is the only way to type a newline",
  );
  assert.match(handler, /event\.preventDefault\(\)/, "a sending Enter must not also insert its newline");
});

test("the send key reuses the submit path rather than opening a second one", () => {
  assert.match(
    keydownHandler(),
    /composer\.requestSubmit\(\)/,
    "requestSubmit goes through the submit listener, so the guards there still run",
  );
  assert.equal(
    js.split('invoke("chat_send"').length - 1,
    1,
    "one place a turn is actually sent, so the empty-line and attach guards cannot be bypassed",
  );
});

// The Chat surface is 420 by 560 points (`main.rs`) and the composer sits under
// the log, so any height the field takes is height the conversation loses. It
// rests at the single line it has always been and a paste scrolls inside it.
function ruleBlock(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = css.match(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`));
  assert.ok(match, `${selector} has no rule of its own`);
  return match[1];
}

test("the composer field rests at one row and scrolls a paste inside itself", () => {
  assert.match(composerForm(), /<textarea\b[^>]*\brows="1"/, "one row at rest, as the input was");

  const field = ruleBlock(".composer textarea");
  assert.match(field, /max-height:/, "a paste cannot push the log off the window");
  assert.match(field, /overflow-y:\s*auto/, "it scrolls inside the field instead");
  assert.match(field, /resize:\s*none/, "and there is no drag handle to take the log's room by hand");
  assert.match(field, /box-sizing:\s*border-box/, "padding sits inside the width flex hands it");
  assert.match(field, /min-width:\s*0/, "#641: a replaced field that cannot shrink paints past its track");
});

test("no rule is still addressed to the input the composer no longer has", () => {
  assert.doesNotMatch(
    css,
    /\.composer input\b/,
    "these stopped applying the moment the element changed",
  );
});

test("the composer textarea has an explicit caret color for webkit2gtk visibility", () => {
  const field = ruleBlock(".composer textarea");
  assert.match(
    field,
    /caret-color:/,
    "webkit2gtk on Linux needs an explicit caret-color; without it the caret is invisible",
  );
});
