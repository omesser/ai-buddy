// Run with `node --test tests/`.
//
// The tab bar is the one part of the Settings page the form description does
// not draw: settings.html writes the five buttons out, and `tabTitles()` says
// what the snapshot expects them to be. A tab added in Rust and not here means
// a tab of the form nobody can reach, and nothing else would say so.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { selectTab, tabTitles } from "../src/settings.js";

// Two files per state. The description keeps the `settings-snapshot-` name and
// the bare `FormDescription` shape that the Rust side's own fixture has, so
// that one drops in here unchanged; the values ride alongside it.
function snapshot(name) {
  const read = (kind) =>
    JSON.parse(readFileSync(new URL(`./fixtures/settings-${kind}-${name}.json`, import.meta.url), "utf8"));
  return { form: read("snapshot"), values: read("values") };
}

const TABS = ["Presence", "Character", "AI", "Privacy", "Development"];
const html = readFileSync(new URL("../src/settings.html", import.meta.url), "utf8");

test("the snapshot carries the five tabs, in order", () => {
  for (const name of ["modelApi", "harnessDriving"]) {
    assert.deepEqual(tabTitles(snapshot(name).form), TABS, name);
  }
});

test("the tab bar is a tablist of buttons over one panel", () => {
  assert.match(html, /<nav[^>]*\brole="tablist"/, "the bar is a nav with the tablist role");
  assert.match(html, /<main[^>]*\brole="tabpanel"/, "the form is the panel the tabs own");

  const tabs = [...html.matchAll(/<button[^>]*\brole="tab"[^>]*>([^<]*)<\/button>/g)].map((match) => match[1]);
  assert.deepEqual(tabs, TABS, "the bar and the form describe different tabs");

  const selected = [...html.matchAll(/\baria-selected="true"/g)];
  assert.equal(selected.length, 1, "exactly one tab opens selected");
});

// A tablist whose tabs name no panel announces as five loose buttons. The
// accessibility tree is the whole argument for leaving AppKit, so the link
// that makes it a tablist is asserted rather than assumed (#706, condition 5).
test("every tab names the panel it opens", () => {
  const controls = [...html.matchAll(/<button[^>]*\brole="tab"[^>]*\baria-controls="([^"]*)"/g)].map(
    (match) => match[1],
  );

  assert.deepEqual(controls, ["set-panel", "set-panel", "set-panel", "set-panel", "set-panel"]);
  assert.match(html, /<main[^>]*\bid="set-panel"/, "the panel the tabs name has to exist");
});

test("the page loads its module as a module and inlines no script", () => {
  assert.match(html, /<script type="module" src="settings\.js"><\/script>/, "the CSP has no unsafe-inline");
  assert.doesNotMatch(html, /<script(?![^>]*\bsrc=)/, "an inline script would be blocked, silently");
  assert.match(html, /<html lang="en" class="chat-ui-minimal">/, "the design class picks the token set");

  const sheets = [...html.matchAll(/<link rel="stylesheet" href="([^"]+)"/g)].map((match) => match[1]);
  assert.deepEqual(sheets, ["chat-ui.css", "settings.css"], "settings.css reads tokens, so it comes second");
});

test("a title picks its tab, and an unknown title picks the first", () => {
  const { form } = snapshot("modelApi");

  assert.equal(selectTab(form, "Presence"), 0);
  assert.equal(selectTab(form, "AI"), 2);
  assert.equal(selectTab(form, "Development"), 4);
  assert.equal(selectTab(form, "Completer"), 0, "a renamed tab opens the first one, never an empty panel");
});
