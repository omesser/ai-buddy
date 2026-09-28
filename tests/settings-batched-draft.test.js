// A tab switch redraws from the snapshot. The draft preserves staged edits
// across those redraws until Apply or Cancel.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { foldDraft, pressFeedback, processResponse, pruneDraft, render } from "../src/settings.js";

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

function draw(values, emit, stage, tab = AI) {
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
  render(root, tab, values, emit, stage);
  const walk = (node) => [node, ...(node.children ?? []).flatMap(walk)];
  const all = root.children.flatMap(walk);
  delete globalThis.document;
  const find = (id) => all.find((node) => node.id === `set-f-${id}` || node.dataset.id === id);
  find.row = (id) => all.find((node) => node.attributes?.["data-row"] === id);
  return find;
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
  key.handlers.blur?.();

  const harness = control("harness");
  harness.value = "Claude Code";
  harness.handlers.change?.();

  assert.deepEqual(staged, [
    ["director_model", "gpt-5"],
    ["director_api_key", "sk-draft"],
    ["harness", "Claude Code"],
  ]);
  assert.deepEqual(emitted, [], "a batched row never writes on blur (#663)");
});

test("AI switches and wake interval wait for Apply and can be discarded", () => {
  const emitted = [];
  let draft = {};
  const control = draw(MODEL_API.values, (payload) => emitted.push(payload), (id, value) => {
    draft[id] = value;
  });
  const director = control.row("director").children[0].children[0];
  director.checked = false;
  director.handlers.change();
  const proactive = control.row("proactive").children[0].children[0];
  proactive.checked = false;
  proactive.handlers.change();
  const piMcp = control.row("pi_project_mcp").children[0].children[0];
  piMcp.checked = false;
  piMcp.handlers.change();
  const wake = control("director_wake_secs");
  wake.value = "240";
  wake.handlers.input();
  wake.handlers.blur?.();

  assert.deepEqual(emitted, []);
  assert.deepEqual(draft, {
    director: false,
    proactive: false,
    pi_project_mcp: false,
    director_wake_secs: "240",
  });
  assert.equal(draw({ ...MODEL_API.values, ...draft }).row("director").children[0].children[0].checked, false);
  draft = foldDraft(draft, { reset: true });
  assert.equal(draw({ ...MODEL_API.values, ...draft }).row("director").children[0].children[0].checked, true);
  assert.equal(draw({ ...MODEL_API.values, ...draft })("director_wake_secs").value, "180");
});

test("Apply and Cancel report a change only when the draft differs from the store", () => {
  const values = MODEL_API.values;
  for (const press of ["director_apply", "director_cancel"]) {
    assert.equal(pressFeedback(press, {}, values), null, press);
    assert.equal(pressFeedback(press, { director: values.director }, values), null, `${press}, toggled back`);
  }
  const toggled = { director: !values.director };
  assert.equal(pressFeedback("director_apply", toggled, values), "Changes applied.");
  assert.equal(pressFeedback("director_cancel", toggled, values), "Changes discarded.");
});

test("a batched row draws the draft it is handed, the key field included", () => {
  const control = draw({ ...MODEL_API.values, director_model: "gpt-5", director_api_key: "sk-draft" });

  assert.equal(control("director_model").value, "gpt-5");
  assert.equal(control("director_api_key").value, "sk-draft");
});

test("registration Harness previews without saving and Cancel restores the picker", () => {
  const emitted = [];
  let draft = {};
  const control = draw(MODEL_API.values, (payload) => emitted.push(payload), (id, value) => {
    draft[id] = value;
  });
  const picker = control("byo_harness");
  picker.value = "hermes";
  picker.handlers.change();

  assert.deepEqual(emitted, [{ pick: "byo_harness", value: "hermes" }]);
  assert.deepEqual(draft, { byo_harness: "hermes" });

  const preview = processResponse({
    action: "preview_byo",
    snippet: "hermes mcp add fidget",
    steps: "Run the command.",
    token: "secret-token",
  });
  draft = foldDraft(draft, preview, emitted[0]);
  const shown = draw({ ...MODEL_API.values, ...draft });
  assert.equal(shown("byo_harness").value, "hermes");
  assert.equal(shown.row("byo_snippet").children[0].textContent, "hermes mcp add fidget");
  assert.equal(shown.row("byo_token").children[0].textContent, "secret-token");

  draft = foldDraft(draft, { reset: true });
  assert.equal(draw({ ...MODEL_API.values, ...draft })("byo_harness").value, "claude");
});

test("a registration preview that lands after Cancel is dropped", () => {
  const emitted = [];
  let draft = {};
  const control = draw(MODEL_API.values, (payload) => emitted.push(payload), (id, value) => {
    draft[id] = value;
  });
  const picker = control("byo_harness");
  picker.value = "hermes";
  picker.handlers.change();
  const [pick] = emitted;

  draft = foldDraft(draft, { reset: true });
  const late = processResponse({ action: "preview_byo", snippet: "hermes mcp add fidget", steps: "Run it.", token: "t" });
  draft = foldDraft(draft, late, pick);

  assert.deepEqual(draft, {});
  assert.deepEqual(emitted, [pick], "a pick writes nothing; only Apply saves");
  const shown = draw({ ...MODEL_API.values, ...draft });
  assert.equal(shown("byo_harness").value, MODEL_API.values.byo_harness);
  assert.equal(shown.row("byo_snippet").children[0].textContent, MODEL_API.values.byo_snippet);
});

test("an unbatched popup saves its pick at once and stages nothing", () => {
  const emitted = [];
  const staged = [];
  const chat = MODEL_API.form.tabs.find((tab) => tab.title === "Chat");
  const control = draw(MODEL_API.values, (payload) => emitted.push(payload), (id) => staged.push(id), chat);
  const select = control("chat_ui");
  const other = select.children.find((option) => option.attributes.value !== select.value);
  select.value = other.attributes.value;
  select.handlers.change();

  assert.deepEqual(emitted, [{ pick: "chat_ui", value: other.attributes.value }]);
  assert.deepEqual(staged, []);
});

// The regression in #995, as a tab switch does it: the page redraws the panel
// from `{ ...snapshot, ...draft }`, so a batched edit survives only if it was
// staged out of the widget and drawn back in.
test("the Pi file row stays hidden until the staged source is Pi", () => {
  const model = draw(MODEL_API.values, () => {}, () => {});
  assert.equal(model.row("pi_project_mcp").hidden, true);

  const pi = draw({ ...MODEL_API.values, harness: "Harness · pi" }, () => {}, () => {});
  const row = pi.row("pi_project_mcp");
  assert.equal(row.hidden, false);
  const status = row.children.find((child) => child.attributes?.class === "set-status");
  assert.match(status.attributes.text ?? status.textContent, /pi-mcp-adapter/);
  assert.match(status.attributes.text ?? status.textContent, /FIDGET_MCP_TOKEN/);
});

test("a staged Model survives the redraw a tab switch performs", () => {
  let draft = {};
  const first = draw(MODEL_API.values, () => {}, (id, value) => (draft = { ...draft, [id]: value }));
  const model = first("director_model");
  model.value = "gpt-5";
  model.handlers.input?.();

  const again = draw({ ...MODEL_API.values, ...draft });

  assert.deepEqual(draft, { director_model: "gpt-5" });
  assert.equal(again("director_model").value, "gpt-5");
  assert.equal(again("director_base_url").value, "https://api.openai.com", "an untouched row still draws the snapshot");
});

test("a Base URL shortcut fill is drawn after the redraw, and Cancel drops it", () => {
  const filled = foldDraft({}, { fill: { id: "director_base_url", value: "https://api.anthropic.com" } });
  const drawnFilled = draw({ ...MODEL_API.values, ...filled });
  assert.equal(drawnFilled("director_base_url").value, "https://api.anthropic.com");

  const cancelled = foldDraft(filled, { reset: true });
  assert.deepEqual(cancelled, {});
  assert.equal(draw({ ...MODEL_API.values, ...cancelled })("director_base_url").value, "https://api.openai.com");
});

test("Clear key blanks only the key's draft entry", () => {
  const draft = { director_api_key: "sk-draft", director_model: "gpt-5" };
  assert.deepEqual(foldDraft(draft, { clearKey: true }), { director_model: "gpt-5" });
  assert.deepEqual(foldDraft(draft, true), draft, "a plain refresh keeps the draft");
  assert.deepEqual(foldDraft(draft, false), draft, "a no-op answer keeps the draft");
});

test("a fresh snapshot prunes the draft entries it already holds", () => {
  const draft = { director_model: "gpt-4o-mini", harness: "Claude Code" };
  assert.deepEqual(pruneDraft(draft, MODEL_API.values), { harness: "Claude Code" });
});
