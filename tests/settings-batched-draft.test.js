// A batched row stays in the widget until Apply (#663), and the page redraws
// the panel from its snapshot on every tab switch. The draft has to live
// outside the widget for the two to agree: `render()` reports each edit to a
// batched row through `stage`, and draws whatever draft value it is handed.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { render } from "../src/settings.js";

function snapshot(name) {
  const read = (kind) =>
    JSON.parse(readFileSync(new URL(`./fixtures/settings-${kind}-${name}.json`, import.meta.url), "utf8"));
  return { form: read("snapshot"), values: read("values") };
}

const MODEL_API = snapshot("modelApi");
const AI = MODEL_API.form.tabs.find((tab) => tab.title === "AI");

function stubDocument() {
  globalThis.document = {
    getElementById: () => null,
    createElement(tag) {
      const node = {
        tagName: tag,
        id: "",
        value: "",
        dataset: {},
        attributes: {},
        children: [],
        style: {},
        handlers: {},
        setAttribute(name, value) {
          node.attributes[name] = value;
        },
        append(...nodes) {
          node.children.push(...nodes);
          for (const child of nodes) {
            if (tag === "select" && child.attributes.selected !== undefined) {
              node.value = child.attributes.value;
            }
          }
        },
        addEventListener(name, handler) {
          node.handlers[name] = handler;
        },
        closest: () => null,
        getRootNode: () => node,
        focus() {},
      };
      return node;
    },
    activeElement: null,
  };
}

function draw(values, emit, stage) {
  const root = {
    children: [],
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children.push(...nodes);
    },
  };
  stubDocument();
  render(root, AI, values, emit, stage);
  const walk = (node) => [node, ...(node.children ?? []).flatMap(walk)];
  const all = root.children.flatMap(walk);
  delete globalThis.document;
  return (id) => all.find((node) => node.id === `set-f-${id}` || node.dataset.id === id);
}

test("a batched row reports each edit through stage and writes nothing on blur", () => {
  const emitted = [];
  const staged = [];
  const control = draw(
    MODEL_API.values,
    (payload) => emitted.push(payload),
    (id, value) => staged.push([id, value]),
  );

  const model = control("director_model");
  model.value = "gpt-5";
  model.handlers.input?.();
  model.handlers.blur?.();

  const key = control("director_api_key");
  key.value = "sk-draft";
  key.handlers.input?.();

  const harness = control("harness");
  harness.value = "Claude Code";
  harness.handlers.change?.();

  assert.deepEqual(staged, [
    ["director_model", "gpt-5"],
    ["director_api_key", "sk-draft"],
    ["harness", "Claude Code"],
  ]);
  assert.deepEqual(
    emitted.filter((payload) => "set_text" in payload && payload.set_text !== "director_api_key"),
    [],
    "a batched row never writes on blur (#663)",
  );
});

test("a batched row draws the draft it is handed, the key field included", () => {
  const control = draw({ ...MODEL_API.values, director_model: "gpt-5", director_api_key: "sk-draft" });

  assert.equal(control("director_model").value, "gpt-5");
  assert.equal(control("director_api_key").value, "sk-draft");
});
