// `tabTitles()` is the list the form description exposes, and `selectTab`
// maps a title onto that list. An unknown title opens the first tab.

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

const TABS = ["Presence", "Character", "AI", "Chat", "Privacy", "Development"];

test("the snapshot carries the tabs, in order", () => {
  for (const name of ["modelApi", "harnessDriving"]) {
    assert.deepEqual(tabTitles(snapshot(name).form), TABS, name);
  }
});

test("a title picks its tab, and an unknown title picks the first", () => {
  const { form } = snapshot("modelApi");

  for (const [index, title] of TABS.entries()) {
    assert.equal(selectTab(form, title), index);
  }
  assert.equal(selectTab(form, "Completer"), 0, "a renamed tab opens the first one, never an empty panel");
});

// DROPPED (#1268): "the tab bar is a tablist of buttons over one panel"
// matches role=tablist in settings.html. selectTab is still called.
// DROPPED (#1268): "every tab names the panel it opens"
// matches aria-controls=set-panel in settings.html.
// DROPPED (#1268): "the page loads its module as a module and inlines no script"
// matches the script tag and stylesheet order in settings.html.
