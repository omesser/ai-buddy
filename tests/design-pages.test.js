// ADR-0011: every published page is Generated or Dated. These pages are
// reachable without the index, so a class string only on the directory is not
// enough, and a Described claim is the class that decision forbids.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const design = (name) =>
  readFileSync(new URL(`../docs/design/${name}`, import.meta.url), "utf8");

function indexEntry(html, href) {
  const hrefAt = html.indexOf(`href="${href}"`);
  assert.ok(hrefAt >= 0, `index lists ${href}`);
  const start = html.lastIndexOf("<li>", hrefAt);
  const end = html.indexOf("</li>", hrefAt);
  assert.ok(start >= 0 && end > start, `index wraps ${href} in an <li>`);
  return html.slice(start, end + "</li>".length);
}

function cls(entry) {
  const match = entry.match(/<div class="cls">([^<]*)<\/div>/);
  assert.ok(match, "entry names its class");
  return match[1];
}

test("index cues entry is the #277 Dated proposal, not a live spec", () => {
  const entry = indexEntry(design("index.html"), "cues.html");
  const named = cls(entry);
  assert.match(named, /Dated proposal/);
  assert.match(named, /#277/);
  assert.doesNotMatch(entry, /The spec the Shell implements/);
});

test("cues.html carries noindex and a visible Dated class line", () => {
  const html = design("cues.html");
  assert.match(html, /<meta name="robots" content="noindex">/);
  assert.match(
    html,
    /Dated proposal · <a href="https:\/\/github\.com\/omesser\/fidget\/issues\/277">#277<\/a> · .+ · hand-written and frozen, not a description of shipped behavior/,
  );
});

test("index chat-mockups entry is the #339 Dated proposal", () => {
  const named = cls(indexEntry(design("index.html"), "chat-mockups.html"));
  assert.match(named, /Dated proposal/);
  assert.match(named, /#339/);
  assert.doesNotMatch(named, /in review on #17/);
});

test("ADR-0011 no longer calls cues.html Described", () => {
  const adr = readFileSync(
    new URL("../docs/adr/0011-generated-or-dated-site-pages.md", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(
    adr,
    /cues\.html` is Described today/,
    "the relabel exit is taken; Consequences must not still name the forbidden class as current",
  );
});

test("chat-mockups.html class line attributes #339", () => {
  const html = design("chat-mockups.html");
  const classLine = html.match(/<p class="class">([\s\S]*?)<\/p>/);
  assert.ok(classLine, "page repeats its class in the header");
  assert.match(classLine[1], /#339/);
  assert.doesNotMatch(classLine[1], /#17/);
});

test("index window-titles-hint entry is the #916 Dated proposal", () => {
  const named = cls(indexEntry(design("index.html"), "window-titles-hint.html"));
  assert.match(named, /Dated proposal/);
  assert.match(named, /#916/);
});

test("window-titles-hint.html carries noindex and a visible Dated class line", () => {
  const html = design("window-titles-hint.html");
  assert.match(html, /<meta name="robots" content="noindex">/);
  assert.match(
    html,
    /Dated proposal · <a href="https:\/\/github\.com\/omesser\/fidget\/issues\/916">#916<\/a> · .+ · hand-written and frozen, not a description of shipped behavior/,
  );
});

// #916 and ADR-0032: the hint names what the fidget knows, never the grant that
// buys it, and never a verb of sight. The Settings mock on the same page is
// allowed to name Screen Recording, so only the hint elements are checked.
// #1076: the design pages show the locked quick-message pill. The tokens are
// the ones src/ ships, so a showcase that drifts back to an underlined link
// fails here without a browser.
test("quick-message showcase matches the overlay", () => {
  const cues = design("cues.html");
  const bubble = design("bubble.html");
  const js = readFileSync(new URL("../src/quick-message.js", import.meta.url), "utf8");
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const css = readFileSync(new URL("../src/main.css", import.meta.url), "utf8");
  const shared = readFileSync(new URL("../src/chat-shared.css", import.meta.url), "utf8");

  assert.match(js, /HOVER_DELAY_MS = 2500/);
  assert.match(js, /DRAG_DISMISS_PX = 4/);
  assert.match(cues, /HOVER_DELAY_MS = 2500/);
  assert.match(cues, /DRAG_DISMISS_PX = 4/);
  assert.match(js, /return "talk to me"/);
  assert.match(cues, /placeholder="talk to me"/);
  assert.match(bubble, /placeholder="talk to me"/);
  assert.match(cues, /id="quick-message-frozen"/);
  assert.match(bubble, /id="quick-message-frozen"/);
  assert.match(cues, /disabled placeholder="Nothing can answer yet"/);
  assert.match(bubble, /disabled placeholder="Nothing can answer yet"/);
  assert.match(bubble, /disabled placeholder="Starting Hermes…"/);
  assert.match(cues, /#2c3340/);
  assert.match(bubble, /#2c3340/);
  assert.match(cues, /\.quick-message-send:disabled \{[^}]*background:\s*#2c3340/s);
  assert.match(bubble, /\.quick-message-send:disabled \{[^}]*background:\s*#2c3340/s);
  assert.match(main, /M2\.2 1\.4 L10\.2 6 L2\.2 10\.6/);
  assert.match(cues, /M2\.2 1\.4 L10\.2 6 L2\.2 10\.6/);
  assert.match(bubble, /M2\.2 1\.4 L10\.2 6 L2\.2 10\.6/);
  assert.match(main, /fill", "#14171e"/);
  assert.match(cues, /fill="#14171e"/);
  assert.match(bubble, /fill="#14171e"/);
  assert.match(shared, /--shared-accent:\s*#5cc9b5/);
  assert.match(cues, /#5cc9b5/);
  assert.match(bubble, /#5cc9b5/);
  assert.match(css, /caret-color:\s*var\(--shared-accent\)/);
  assert.match(cues, /caret-color:\s*var\(--qm-accent\)/);
  assert.match(bubble, /caret-color:\s*var\(--qm-accent\)/);
  assert.match(css, /background:\s*var\(--shared-accent\)/);
  assert.match(cues, /background:\s*var\(--qm-accent\)/);
  assert.match(bubble, /background:\s*var\(--qm-accent\)/);
  const noTail = /\.quick-message::before,\s*\.quick-message::after\s*\{\s*content:\s*none/s;
  assert.match(css, /\.bubble\.quick-message::before,\s*\.bubble\.quick-message::after\s*\{\s*content:\s*none/s);
  assert.match(cues, noTail);
  assert.match(bubble, noTail);
  assert.match(css, /380ms cubic-bezier\(0\.16, 1, 0\.3, 1\)/);
  assert.match(cues, /380ms cubic-bezier\(0\.16, 1, 0\.3, 1\)/);
  assert.match(bubble, /380ms cubic-bezier\(0\.16, 1, 0\.3, 1\)/);
  assert.match(css, /translateY\(-2px\) scale\(1\.02\)/);
  assert.match(cues, /translateY\(-2px\) scale\(1\.02\)/);
  assert.match(bubble, /translateY\(-2px\) scale\(1\.02\)/);

  assert.match(cues, /Left during the dwell/);
  assert.match(cues, /does not dismiss/);
  assert.match(cues, /Escape does not dismiss/);
  assert.match(cues, /selects text/);
  assert.match(cues, /at least 4px/);
  assert.match(cues, /double-click is still Summon/);
  assert.match(cues, /no walk-cycle frames/);
  assert.match(cues, /Engine still lets one leave/);
  assert.doesNotMatch(cues, /class="bubble-more"|Open chat/);

  assert.match(bubble, /no tail/i);
  assert.match(bubble, /2\.5s/);
  assert.match(bubble, /at least 4px/);
  assert.match(bubble, /does not dismiss/);
  assert.match(bubble, /selects\s+text/);
  assert.match(bubble, /no\s+locomotion/);
  assert.match(bubble, /cues\.html#quick-message-spec/);
  assert.doesNotMatch(bubble, /class="bubble-more"|Open chat/);
});

test("index names the quick-message on the cues and bubble pages", () => {
  const index = design("index.html");
  const cues = indexEntry(index, "cues.html");
  const bubble = indexEntry(index, "bubble.html");
  assert.match(cues, /2\.5s/);
  assert.match(cues, /quick-message/);
  assert.match(bubble, /no tail/);
  assert.match(bubble, /talk to me/);
  assert.match(bubble, /Nothing can answer yet/);
  assert.match(cls(cues), /Dated proposal/);
  assert.match(cls(cues), /#277/);
  assert.match(cls(bubble), /#441/);
});

test("window-titles-hint.html hint copy names no grant, no Capture and no sight", () => {
  const html = design("window-titles-hint.html");
  const hint = (id) => {
    const match = html.match(new RegExp(`id="${id}"[^>]*>([\\s\\S]*?)<div class="row">`));
    assert.ok(match, `page has the ${id} hint`);
    return match[1];
  };
  for (const copy of [hint("notice"), hint("bubble")]) {
    assert.doesNotMatch(copy, /Screen Recording|ScreenCast|screenshot|capture/i);
    assert.doesNotMatch(copy, /\b(see|sees|watch|watches|look|looks)\b/i);
    assert.match(copy, /where your windows are, not what they are/);
  }
});
