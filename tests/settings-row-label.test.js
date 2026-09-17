// A row's name reaches the accessibility tree once, because its control sits
// inside its label. WebKit publishes a label holding nothing but text as two
// nodes carrying the same string, and verify-settings-macos.sh reads the line
// after the name as the row's control, so the copy answered for the field and
// three rows a user can edit read as frozen (#706).

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { render } from "../src/settings.js";

// Enough of a document for the renderer: the page never reads layout back, so
// a node is its tag, its attributes and what hangs under it.
function element(tag) {
  return {
    tagName: tag,
    attributes: {},
    children: [],
    textContent: "",
    dataset: {},
    setAttribute(name, value) {
      this.attributes[name] = String(value);
    },
    getAttribute(name) {
      return this.attributes[name] ?? null;
    },
    addEventListener() {},
    append(...nodes) {
      this.children.push(...nodes);
    },
    replaceChildren(...nodes) {
      this.children = nodes;
    },
  };
}
globalThis.document = { createElement: element };

function snapshot(name) {
  const read = (kind) =>
    JSON.parse(readFileSync(new URL(`./fixtures/settings-${kind}-${name}.json`, import.meta.url), "utf8"));
  return { form: read("snapshot"), values: read("values") };
}

function descendants(node) {
  return node.children.flatMap((child) => [child, ...descendants(child)]);
}

const CONTROLS = ["input", "select", "textarea"];

function page(name, title) {
  const state = snapshot(name);
  const tab = state.form.tabs.find((candidate) => candidate.title === title);
  assert.ok(tab, `the snapshot carries no ${title} tab`);
  const root = element("main");
  render(root, tab, state.values);
  return descendants(root);
}

const STATES = [
  ["Model API drives", "modelApi"],
  ["a Harness drives", "harnessDriving"],
];

test("every label holds the control it names", () => {
  for (const [when, name] of STATES) {
    for (const title of ["Presence", "Character", "AI", "Privacy", "Development"]) {
      for (const label of page(name, title).filter((node) => node.tagName === "label")) {
        const control = descendants(label).find((node) => CONTROLS.includes(node.tagName));
        assert.ok(control, `a label in ${title} holds no control when ${when}`);
        if (label.getAttribute("for")) {
          assert.equal(control.id, label.getAttribute("for"), `${title} when ${when}`);
        }
      }
    }
  }
});
