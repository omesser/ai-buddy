import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

test("Settings reveal shows Privacy tab with use_window_names row after hint 'Open Settings'", async () => {
  const form = JSON.parse(readFileSync(new URL("./fixtures/settings-snapshot-modelApi.json", import.meta.url), "utf8"));
  const values = JSON.parse(readFileSync(new URL("./fixtures/settings-values-modelApi.json", import.meta.url), "utf8"));

  const WINDOW_NAMES_ROW_ID = "consent_screen_recording";

  let privacyTabSelected = false;

  const mockFooter = {
    replaceChildren() { this.children = []; },
    append(...nodes) { this.children.push(...nodes); },
    children: [],
    style: {},
    querySelectorAll: () => []
  };

  const mockPanel = {
    replaceChildren() { this.children = []; },
    append(...nodes) { this.children.push(...nodes); },
    appendChild(node) { this.children.push(node); },
    children: [],
    setAttribute: () => {},
    querySelector: (selector) => {
      if (selector.startsWith('[data-row=')) {
        const walk = (node) => [node, ...(node.children ?? []).flatMap(walk)];
        const all = this.children.flatMap(walk);
        const rowId = selector.match(/\[data-row="([^"]+)"\]/)?.[1];
        const found = all.find(n => n.dataset?.row === rowId);
        if (found) {
          return {
            scrollIntoView: () => {},
            hasAttribute: () => false,
            focus: () => {},
            set tabIndex(val) {}
          };
        }
      }
      return null;
    },
    querySelectorAll: () => []
  };

  const TAB_TITLES = ["Presence", "Character", "AI", "Chat", "Privacy", "Development"];
  const mockTabs = TAB_TITLES.map(title => ({
    textContent: title,
    setAttribute: (attr, val) => {
      if (attr === "aria-selected" && title === "Privacy" && val === "true") {
        privacyTabSelected = true;
      }
    },
    addEventListener: () => {}
  }));

  const mockTablist = {
    querySelectorAll: () => mockTabs
  };

  const mockDocument = {
    documentElement: {
      dataset: {},
      classList: { toString: () => "" }
    },
    readyState: "complete",
    querySelector: (selector) => {
      if (selector === '[role="tablist"]') return mockTablist;
      if (selector === '[role="tabpanel"]') return mockPanel;
      if (selector === ".set-feedback") return null;
      return null;
    },
    querySelectorAll: () => [],
    addEventListener: () => {},
    createElement: (tag) => {
      const node = {
        tagName: tag,
        textContent: "",
        className: "",
        style: {},
        dataset: {},
        attributes: {},
        children: [],
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
        appendChild(child) {
          node.children.push(child);
        },
        addEventListener: () => {},
        replaceChildren() {
          node.children = [];
        },
        querySelector: () => null,
        querySelectorAll: () => [],
        closest: () => null,
        getRootNode: () => node,
        focus: () => {},
        hasAttribute: () => false
      };
      return node;
    },
    getElementById: (id) => {
      if (id === "set-footer") return mockFooter;
      return null;
    },
    activeElement: null
  };

  const mockWindow = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd, payload) => {
        if (cmd === "settings_event") {
          return { action: "nothing" };
        }
        return { action: "nothing" };
      }
    },
    __TAURI__: {
      core: {
        invoke: async (cmd) => {
          if (cmd === "settings_snapshot") {
            return {
              form,
              view: { ...values, [WINDOW_NAMES_ROW_ID]: false },
              reveal: { tab: "Privacy", row: WINDOW_NAMES_ROW_ID }
            };
          }
          return { action: "nothing" };
        }
      },
      event: { listen: async () => () => {} },
      webviewWindow: {
        getCurrentWebviewWindow: () => ({ startDragging: () => {}, close: () => {} })
      }
    },
    addEventListener: () => {},
    matchMedia: () => ({ matches: false, addEventListener: () => {} })
  };

  globalThis.document = mockDocument;
  globalThis.window = mockWindow;

  const timestamp = Date.now();
  await import(`../src/settings.js?t=${timestamp}`);

  await new Promise(resolve => setTimeout(resolve, 100));

  const walk = (node) => [node, ...(node.children ?? []).flatMap(walk)];
  const allNodes = mockPanel.children.flatMap(walk);
  const windowNamesRow = allNodes.find(n => n.dataset?.row === WINDOW_NAMES_ROW_ID);

  assert.ok(privacyTabSelected, "Privacy tab should be selected (aria-selected='true') after reveal");

  const privacyTab = form.tabs.find(t => t.title === "Privacy");
  const hasWindowNamesRow = privacyTab.sections.some(s =>
    s.rows.some(r => r.id === WINDOW_NAMES_ROW_ID)
  );
  assert.ok(hasWindowNamesRow, `Privacy tab form data should contain row with id="${WINDOW_NAMES_ROW_ID}"`);

  delete globalThis.document;
  delete globalThis.window;
});
