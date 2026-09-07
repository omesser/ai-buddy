// Run with `node --test tests/`.
//
// ADR-0011: every published page is Generated or Dated. These pages are
// reachable without the index, so a class string only on the directory is
// not enough — and a Described claim ("the spec the Shell implements") is
// the class that decision forbids.

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

test("chat-mockups.html class line attributes #339", () => {
  const html = design("chat-mockups.html");
  const classLine = html.match(/<p class="class">([\s\S]*?)<\/p>/);
  assert.ok(classLine, "page repeats its class in the header");
  assert.match(classLine[1], /#339/);
  assert.doesNotMatch(classLine[1], /#17/);
});
