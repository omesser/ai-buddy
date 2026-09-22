// A live accessibility dump asks which rows are the user's to edit for the AI
// source in force. `controls()` is the same flat list in render order, so the
// same questions are asked of the fixtures.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { controls } from "../src/settings.js";

// Two files per state. The description keeps the `settings-snapshot-` name and
// the bare `FormDescription` shape that the Rust side's own fixture has, so
// that one drops in here unchanged; the values ride alongside it.
function snapshot(name) {
  const read = (kind) =>
    JSON.parse(readFileSync(new URL(`./fixtures/settings-${kind}-${name}.json`, import.meta.url), "utf8"));
  return { form: read("snapshot"), values: read("values") };
}

const MODEL_API = snapshot("modelApi");
const HARNESS = snapshot("harnessDriving");
const STATES = [
  ["Model API drives", MODEL_API],
  ["a Harness drives", HARNESS],
];

const TABS = ["Presence", "Character", "AI", "Privacy", "Development"];

function tab(state, title) {
  const found = state.form.tabs.find((candidate) => candidate.title === title);
  assert.ok(found, `the snapshot carries no ${title} tab`);
  return controls(found, state.values);
}

const headings = (list) => list.filter((control) => control.role === "heading").map((control) => control.label);
const byId = (list, id) => list.find((control) => control.id === id);
const byLabel = (list, label) =>
  list.find((control) => control.role !== "heading" && control.label === label);

// The script's four section names. It greps for exactly these, so the BYO
// section between "AI source" and "Model / API" is invisible to it, and the
// order it reports is the order of the four that are left.
const AI_SECTIONS = ["AI", "AI source", "Model / API", "Last user turn"];

test("AI tab section order is AI, AI source, Model / API, Last user turn", () => {
  for (const [when, state] of STATES) {
    const order = headings(tab(state, "AI")).filter((heading) => AI_SECTIONS.includes(heading));
    assert.deepEqual(order, AI_SECTIONS, when);
  }
});

const HTTP_ROWS = ["Base URL", "Model", "API key", "Clear key"];

test("the HTTP rows are the user's when Model API drives", () => {
  const list = tab(MODEL_API, "AI");
  for (const label of HTTP_ROWS) {
    assert.equal(byLabel(list, label).frozen, false, label);
  }
  assert.equal(byId(list, "harness").value, "Model API");
});

test("the HTTP rows freeze when a Harness drives", () => {
  const list = tab(HARNESS, "AI");
  for (const label of HTTP_ROWS) {
    assert.equal(byLabel(list, label).frozen, true, `${label} must be frozen (#452)`);
  }
  assert.equal(byId(list, "harness").value, "Harness · claude");
  assert.equal(byId(list, "director_apply").frozen, false, "Apply stays live, or the source cannot be changed back");
});

// The picker carries no label of its own, so it is addressed the way the
// script addresses it: the popup nearest above the Base URL row in render
// order. A picker live over a frozen field is the bug this catches.
function endpointPicker(list) {
  const at = list.findIndex((control) => control.label === "Base URL");
  assert.notEqual(at, -1, "the AI tab has no Base URL row");
  const above = list.slice(0, at).filter((control) => control.role === "popup");
  return above[above.length - 1];
}

test("the endpoint picker sits over Base URL and freezes with it", () => {
  assert.equal(endpointPicker(tab(MODEL_API, "AI")).id, "director_base_url_pick");
  assert.equal(endpointPicker(tab(MODEL_API, "AI")).frozen, false);
  assert.equal(endpointPicker(tab(HARNESS, "AI")).frozen, true);
});

// `has_line`, against the labels and values an accessibility dump would show.
const spoken = (list) => list.map((control) => `${control.label ?? ""}\n${control.value ?? ""}`).join("\n");

test("the state line says the HTTP endpoint is the AI brain", () => {
  assert.equal(spoken(tab(MODEL_API, "AI")).includes("The HTTP endpoint below is the AI brain."), true);
});

test('"the Director\'s mind" stays out of the state line', () => {
  for (const [when, state] of STATES) {
    assert.equal(spoken(tab(state, "AI")).includes("is the Director's mind"), false, when);
  }
});

const HEADINGS = {
  Presence: ["Do Not Disturb", "Hide", "Launch"],
  Character: ["Character", "Instances"],
  AI: ["AI", "AI source", "Point a Harness you run yourself at ai-buddy", "Model / API", "Last user turn"],
  Privacy: ["What the buddy can see", "Excluded applications", "Memory File"],
  Development: ["Traces", "Blank AI", "HTTP limits", "Harness attachment"],
};

test("every tab draws its sections, whichever source drives", () => {
  for (const [when, state] of STATES) {
    for (const title of TABS) {
      assert.deepEqual(headings(tab(state, title)), HEADINGS[title], `${title} when ${when}`);
    }
  }
});

test("no row in either snapshot is one this renderer has no case for", () => {
  const unrendered = [];
  for (const [when, state] of STATES) {
    for (const title of TABS) {
      for (const control of tab(state, title)) {
        if (control.role === "unknown") unrendered.push(`${control.label} in ${title} when ${when}`);
      }
    }
  }
  assert.deepEqual(unrendered, [], "a FormRow variant reached the page with no case in controls()");
});

// Labels belong to the form and values to the file, so a change of AI source
// moves `frozen` and the values and nothing else. A label that moved with the
// source would be a second description of the form, drifting from the first.
const AI_ROWS = [
  ["heading", null, "AI"],
  ["checkbox", "director", "AI on"],
  ["checkbox", "ambient", "Ambient session wakes"],
  ["textfield", "director_wake_secs", "First wake, in seconds"],
  ["button", "new_session", "Start new session"],
  ["heading", null, "AI source"],
  ["popup", "harness", "AI source"],
  ["textfield", "harness_command", "Custom command line"],
  ["statictext", "harness_state", null],
  ["heading", null, "Point a Harness you run yourself at ai-buddy"],
  ["popup", "byo_harness", "Harness"],
  ["statictext", "byo_snippet", null],
  ["button", "byo_copy", "Copy"],
  ["statictext", "byo_token", null],
  ["button", "byo_copy_token", "Copy"],
  ["statictext", "byo_steps", null],
  ["heading", null, "Model / API"],
  ["popup", "director_base_url_pick", null],
  ["textfield", "director_base_url", "Base URL"],
  ["textfield", "director_model", "Model"],
  ["securefield", "director_api_key", "API key"],
  ["button", "clear_key", "Clear key"],
  ["button", "director_apply", "Apply"],
  ["button", "director_cancel", "Cancel"],
  ["heading", null, "Last user turn"],
  ["statictext", "payload", null],
];

test("labels are the same under both sources; only frozen and the values move", () => {
  const shape = (list) => list.map((control) => [control.role, control.id, control.label]);

  assert.deepEqual(shape(tab(MODEL_API, "AI")), AI_ROWS);
  for (const title of TABS) {
    assert.deepEqual(shape(tab(HARNESS, title)), shape(tab(MODEL_API, title)), title);
  }
});

test("Checkbox, TextField, SecureField, Popup and InspectBlock keep their kind", () => {
  const list = tab(MODEL_API, "AI");

  assert.deepEqual(byId(list, "director"), {
    role: "checkbox",
    id: "director",
    label: "AI on",
    value: true,
    frozen: false,
  });
  assert.deepEqual(byId(list, "director_base_url"), {
    role: "textfield",
    id: "director_base_url",
    label: "Base URL",
    value: "https://api.openai.com",
    frozen: false,
  });
  assert.deepEqual(byId(list, "director_api_key"), {
    role: "securefield",
    id: "director_api_key",
    label: "API key",
    value: "",
    frozen: false,
  });
  assert.deepEqual(byId(list, "harness"), {
    role: "popup",
    id: "harness",
    label: "AI source",
    value: "Model API",
    frozen: false,
  });
  assert.deepEqual(byId(list, "harness_state"), {
    role: "statictext",
    id: "harness_state",
    label: null,
    value: "Not attached. The HTTP endpoint below is the AI brain.",
    frozen: true,
  });
});

test("a List is one row and one dismiss button per item", () => {
  const instances = tab(MODEL_API, "Character").filter((control) => control.id === "instances");

  assert.deepEqual(instances, [
    { role: "statictext", id: "instances", label: "BMO (bmo)", value: "BMO (bmo)", frozen: false },
    { role: "button", id: "instances", label: "Dismiss", value: "BMO (bmo)", frozen: false },
    { role: "statictext", id: "instances", label: "Boo (ghost)", value: "Boo (ghost)", frozen: false },
    { role: "button", id: "instances", label: "Dismiss", value: "Boo (ghost)", frozen: false },
  ]);
});

test("a List of Instance rows labels each Name (character)", () => {
  const values = {
    ...MODEL_API.values,
    instances: [
      { id: "bmo-1", name: "BMO", character: "bmo", prompt: "" },
      { id: "ghost-1", name: "Boo", character: "ghost", prompt: "boo" },
    ],
  };
  const instances = controls(
    MODEL_API.form.tabs.find((candidate) => candidate.title === "Character"),
    values,
  ).filter((control) => control.id === "instances");

  assert.deepEqual(instances, [
    { role: "statictext", id: "instances", label: "BMO (bmo)", value: "BMO (bmo)", frozen: false },
    { role: "button", id: "instances", label: "Dismiss", value: "BMO (bmo)", frozen: false },
    { role: "statictext", id: "instances", label: "Boo (ghost)", value: "Boo (ghost)", frozen: false },
    { role: "button", id: "instances", label: "Dismiss", value: "Boo (ghost)", frozen: false },
  ]);
});

test("a Multiline keeps its lines and is live while the form says editable", () => {
  assert.deepEqual(byId(tab(MODEL_API, "Privacy"), "excluded"), {
    role: "textarea",
    id: "excluded",
    label: null,
    value: "Keynote\nzoom.us",
    frozen: false,
  });
});

test("an InspectPath is a path and nothing else", () => {
  assert.deepEqual(byId(tab(MODEL_API, "Privacy"), "memory_path"), {
    role: "statictext",
    id: "memory_path",
    label: null,
    value: "/Users/fixture/Library/Application Support/ai-buddy/memory",
    frozen: true,
  });
});

test("a Composite spreads into one control per member, its text field included", () => {
  const spawn = tab(MODEL_API, "Character").filter((control) =>
    ["new_name", "new_character", "spawn"].includes(control.id),
  );

  assert.deepEqual(spawn, [
    { role: "textfield", id: "new_name", label: null, value: "", frozen: false },
    { role: "popup", id: "new_character", label: null, value: "", frozen: false },
    { role: "button", id: "spawn", label: "New", value: null, frozen: false },
  ]);
});

// No DOM in this suite, so which elements `render()` builds can only be read
// off its source. A <select> hand-rolled out of divs passes every assertion
// above and is unreachable by a screen reader.
const source = readFileSync(new URL("../src/settings.js", import.meta.url), "utf8");

test("the controls are the platform's own elements", () => {
  for (const tag of ["select", "option", "details", "summary", "textarea", "ul", "li"]) {
    assert.match(source, new RegExp(`el\\("${tag}"`), `${tag} is not what render() builds`);
  }
  assert.match(source, /type: "checkbox"/, "a checkbox is an input, not a styled div");
  assert.match(source, /type: "password"/, "a secure field is an input, not a masked text box");
  assert.doesNotMatch(source, /role: "(button|checkbox|combobox|textbox)"/, "no element wears a role it is not");
  assert.match(source, /if \(!row\.batched\)/, "batched source rows wait for Apply");
  assert.match(source, /payload\.draft = directorDraft/, "Apply sends the widget draft");
});

// Two stylesheets on one window, and the second one wins: the prefix is what
// makes a collision with the Chat surface impossible rather than merely avoided.
const css = readFileSync(new URL("../src/settings.css", import.meta.url), "utf8");

test("every selector in settings.css is prefixed .set-", () => {
  const selectors = css
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .split("}")
    .map((block) => block.split("{")[0].trim())
    .filter(Boolean)
    .flatMap((group) => group.split(",").map((one) => one.trim()))
    .filter(Boolean);

  assert.deepEqual(
    selectors.filter((selector) => !selector.startsWith(".set-")),
    [],
    "an unprefixed selector reaches every other surface sharing this window",
  );
});

test("handleEvent returns false for nothing response", async () => {
  const { handleEvent } = await import("../src/settings.js");

  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async () => ({ action: "nothing" }),
    },
  };

  const result = await handleEvent({ press: "director_apply" });
  assert.equal(result, false);

  delete globalThis.window;
});

test("handleEvent returns true for refresh response", async () => {
  const { handleEvent } = await import("../src/settings.js");

  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async () => ({ action: "refresh" }),
    },
  };

  const result = await handleEvent({ pick: "character", value: "bmo" });
  assert.equal(result, true);

  delete globalThis.window;
});

test("handleEvent returns fill object for fill response", async () => {
  const { handleEvent } = await import("../src/settings.js");

  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async () => ({
        action: "fill",
        id: "director_base_url",
        value: "https://api.openai.com",
      }),
    },
  };

  const result = await handleEvent({
    pick: "director_base_url_pick",
    value: "OpenAI",
    fills: { row: "director_base_url" },
  });

  assert.deepEqual(result, {
    fill: {
      id: "director_base_url",
      value: "https://api.openai.com",
    },
  });

  delete globalThis.window;
});

test("handleEvent returns reset object for reset response", async () => {
  const { handleEvent } = await import("../src/settings.js");

  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async () => ({ action: "reset" }),
    },
  };

  const result = await handleEvent({ press: "director_cancel" });
  assert.deepEqual(result, { reset: true });

  delete globalThis.window;
});

test("handleEvent returns clearKey object for clear_key response", async () => {
  const { handleEvent } = await import("../src/settings.js");

  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async () => ({ action: "clear_key" }),
    },
  };

  const result = await handleEvent({ press: "director_api_key_clear" });
  assert.deepEqual(result, { clearKey: true });

  delete globalThis.window;
});

test("director_actions renders to footer when one exists", async () => {
  const { render } = await import("../src/settings.js");

  const mockFooter = {
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children = this.children || [];
      this.children.push(...nodes);
    },
    children: [],
    style: {},
  };

  const mockRoot = {
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children = this.children || [];
      this.children.push(...nodes);
    },
    children: [],
  };

  globalThis.document = {
    getElementById(id) {
      return id === "set-footer" ? mockFooter : null;
    },
    createElement(tag) {
      const node = {
        tagName: tag,
        id: "",
        textContent: "",
        dataset: {},
        attributes: {},
        children: [],
        style: {},
        setAttribute(name, value) {
          node.attributes[name] = value;
          if (name.startsWith("data-")) {
            const key = name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
            node.dataset[key] = value;
          }
        },
        append(...nodes) {
          node.children.push(...nodes);
        },
        addEventListener() {},
        closest() {
          return null;
        },
        getRootNode() {
          return node;
        },
      };
      return node;
    },
  };

  const tab = MODEL_API.form.tabs.find((t) => t.title === "AI");
  render(mockRoot, tab, MODEL_API.values);

  const footerHasDirectorActions = mockFooter.children.some(
    (node) => node.dataset && node.dataset.row === "director_actions",
  );

  assert.ok(
    mockFooter.children.length > 0,
    `footer should have children, got ${mockFooter.children.length} children`,
  );
  assert.ok(footerHasDirectorActions, "director_actions should be in footer when footer element exists");

  delete globalThis.document;
});

test("non-AI tabs hide footer when it has no content (Bug 1)", async () => {
  const { render } = await import("../src/settings.js");

  const mockFooter = {
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children = this.children || [];
      this.children.push(...nodes);
    },
    children: [],
    style: {},
  };

  const mockRoot = {
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children = this.children || [];
      this.children.push(...nodes);
    },
    children: [],
  };

  globalThis.document = {
    getElementById(id) {
      return id === "set-footer" ? mockFooter : null;
    },
    createElement(tag) {
      const node = {
        tagName: tag,
        id: "",
        textContent: "",
        dataset: {},
        attributes: {},
        children: [],
        style: {},
        setAttribute(name, value) {
          node.attributes[name] = value;
          if (name.startsWith("data-")) {
            const key = name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
            node.dataset[key] = value;
          }
        },
        append(...nodes) {
          node.children.push(...nodes);
        },
        addEventListener() {},
        closest() {
          return null;
        },
        getRootNode() {
          return node;
        },
      };
      return node;
    },
  };

  const NON_AI_TABS = ["Presence", "Character", "Privacy", "Development"];

  for (const tabTitle of NON_AI_TABS) {
    mockFooter.children = [];
    mockFooter.style = {};
    const tab = MODEL_API.form.tabs.find((t) => t.title === tabTitle);
    render(mockRoot, tab, MODEL_API.values);

    assert.equal(
      mockFooter.children.length,
      0,
      `${tabTitle} tab footer should have no children`,
    );
    assert.equal(
      mockFooter.style.display,
      "none",
      `${tabTitle} tab footer should be hidden (display: none)`,
    );
  }

  delete globalThis.document;
});

test("AI tab shows footer with Apply and Cancel buttons", async () => {
  const { render } = await import("../src/settings.js");

  const mockFooter = {
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children = this.children || [];
      this.children.push(...nodes);
    },
    children: [],
    style: {},
  };

  const mockRoot = {
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children = this.children || [];
      this.children.push(...nodes);
    },
    children: [],
  };

  globalThis.document = {
    getElementById(id) {
      return id === "set-footer" ? mockFooter : null;
    },
    createElement(tag) {
      const node = {
        tagName: tag,
        id: "",
        textContent: "",
        dataset: {},
        attributes: {},
        children: [],
        style: {},
        setAttribute(name, value) {
          node.attributes[name] = value;
          if (name.startsWith("data-")) {
            const key = name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
            node.dataset[key] = value;
          }
        },
        append(...nodes) {
          node.children.push(...nodes);
        },
        addEventListener() {},
        closest() {
          return null;
        },
        getRootNode() {
          return node;
        },
      };
      return node;
    },
  };

  const tab = MODEL_API.form.tabs.find((t) => t.title === "AI");
  render(mockRoot, tab, MODEL_API.values);

  assert.ok(mockFooter.children.length > 0, "AI tab footer should have children");
  assert.notEqual(mockFooter.style.display, "none", "AI tab footer should be visible");

  delete globalThis.document;
});

test("showError clears and hides footer to prevent empty-payload wipes (Bug 2)", async () => {
  const { showError } = await import("../src/settings.js");

  const mockPanel = {
    replaceChildren() {
      this.children = [];
    },
    appendChild(node) {
      this.children = this.children || [];
      this.children.push(node);
    },
    children: [],
  };

  const mockFooter = {
    replaceChildren() {
      this.children = [];
    },
    append(...nodes) {
      this.children = this.children || [];
      this.children.push(...nodes);
    },
    children: [{ tagName: "button", textContent: "Apply" }],
    style: {},
  };

  const mockDocument = {
    createElement(tag) {
      return {
        tagName: tag,
        className: "",
        style: {},
        textContent: "",
        appendChild() {},
        addEventListener() {},
      };
    },
    querySelector(selector) {
      if (selector === '[role="tabpanel"]') return mockPanel;
      return null;
    },
    getElementById(id) {
      return id === "set-footer" ? mockFooter : null;
    },
  };

  globalThis.document = mockDocument;

  mockFooter.children = [{ tagName: "button", textContent: "Apply" }];
  mockFooter.style = {};

  showError("Could not save changes", () => {});

  assert.equal(mockFooter.children.length, 0, "footer children should be cleared after showError");
  assert.equal(mockFooter.style.display, "none", "footer should be hidden after showError");

  delete globalThis.document;
});
