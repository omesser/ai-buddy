import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { composerPlaceholder } from "../src/chat-placeholder.js";

const html = readFileSync(new URL("../src/chat.html", import.meta.url), "utf8");
const js = readFileSync(new URL("../src/chat.js", import.meta.url), "utf8");

test("when something can answer, the composer asks the Instance by name", () => {
  const text = composerPlaceholder({
    name: "bmo",
    configured: true,
    enabled: true,
  });

  assert.equal(text, "Ask bmo…");
});

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

test("the composer markup does not ship Say something as the live placeholder", () => {
  assert.doesNotMatch(html, /placeholder="Say something"/);
});

test("unanswerable empty-state copy is unchanged", () => {
  assert.match(html, /id="landing"/);
  assert.match(html, /id="empty-http"/);
  assert.match(html, /Nothing to answer with/);
  assert.match(html, /id="empty-http-off"/);
  assert.match(html, /Answers are switched off/);
});

test("chat.js asks the helper for the composer placeholder", () => {
  assert.match(js, /import \{ composerPlaceholder \} from "\.\/chat-placeholder\.js"/);
  assert.equal(js.split("composerPlaceholder(opening)").length - 1, 2);
  assert.doesNotMatch(js, /Ask \$\{opening\.name\}…/);
  assert.doesNotMatch(js, /"Nothing can answer yet"/);
});
