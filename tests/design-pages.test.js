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
    /Dated proposal · <a href="https:\/\/github\.com\/omesser\/ai-buddy\/issues\/277">#277<\/a> · .+ · hand-written and frozen, not a description of shipped behavior/,
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
    /Dated proposal · <a href="https:\/\/github\.com\/omesser\/ai-buddy\/issues\/916">#916<\/a> · .+ · hand-written and frozen, not a description of shipped behavior/,
  );
});

// #916 and ADR-0032: the hint names what the buddy knows, never the grant that
// buys it, and never a verb of sight. The Settings mock on the same page is
// allowed to name Screen Recording, so only the hint elements are checked.
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
