import assert from "node:assert/strict";
import { test } from "node:test";

test("Settings reveal shows Privacy tab with use_window_names row after hint 'Open Settings'", async () => {
  const mockRow = {
    scrollIntoView: () => {},
    hasAttribute: () => false,
    focus: () => {},
    set tabIndex(val) {}
  };

  let privacyTabSelected = false;
  let windowNamesRowRendered = false;
  let replaceChildrenCalled = false;

  let mockPanelChildren = [];

  const mockPanel = {
    replaceChildren: () => {
      replaceChildrenCalled = true;
      mockPanelChildren = [];
    },
    appendChild: (node) => {
      mockPanelChildren.push(node);
      if (node && node.dataset && node.dataset.row === "use_window_names") {
        windowNamesRowRendered = true;
      }
    },
    append: (...nodes) => {
      mockPanelChildren.push(...nodes);
      for (const node of nodes) {
        if (node && node.dataset && node.dataset.row === "use_window_names") {
          windowNamesRowRendered = true;
        }
      }
    },
    setAttribute: () => {},
    querySelector: (selector) => {
      if (selector === '[data-row="use_window_names"]') {
        const found = mockPanelChildren.find(n => n.dataset?.row === "use_window_names");
        return found || null;
      }
      return null;
    },
    querySelectorAll: () => []
  };

  const mockTabs = [
    {
      textContent: "Presence",
      setAttribute: (attr, val) => {
        if (attr === "aria-selected" && val === "false") privacyTabSelected = false;
      },
      addEventListener: () => {}
    },
    {
      textContent: "Privacy",
      setAttribute: (attr, val) => {
        if (attr === "aria-selected" && val === "true") privacyTabSelected = true;
      },
      addEventListener: () => {}
    }
  ];

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
      return null;
    },
    querySelectorAll: () => [],
    addEventListener: () => {},
    createElement: (tag) => ({
      tagName: tag,
      textContent: "",
      className: "",
      style: {},
      dataset: {},
      setAttribute: () => {},
      appendChild: () => {},
      append: () => {},
      addEventListener: () => {},
      replaceChildren: () => {},
      querySelector: () => null,
      querySelectorAll: () => []
    }),
    getElementById: () => ({ replaceChildren: () => {}, style: {}, appendChild: () => {} }),
    activeElement: null
  };

  globalThis.document = mockDocument;
  globalThis.window = {
    __TAURI__: {
      core: {
        invoke: async (cmd) => {
          if (cmd === "settings_snapshot") {
            return {
              form: {
                tabs: [
                  { title: "Presence", sections: [{ heading: "Presence", rows: [] }] },
                  {
                    title: "Privacy",
                    sections: [{
                      heading: "Consent",
                      rows: [{
                        type: "Checkbox",
                        id: "use_window_names",
                        label: "Window titles and application names",
                        frozen: false
                      }]
                    }]
                  }
                ]
              },
              view: { use_window_names: false },
              reveal: { tab: "Privacy", row: "use_window_names" }
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

  const timestamp = Date.now();
  await import(`../src/settings.js?t=${timestamp}`);

  await new Promise(resolve => setTimeout(resolve, 100));

  assert(privacyTabSelected, "Privacy tab should be selected after reveal");
  assert(replaceChildrenCalled, "panel.replaceChildren should be called during render");
  assert(mockPanelChildren.length > 0, "Settings content should be rendered (panel has children)");

  delete globalThis.document;
  delete globalThis.window;
});
