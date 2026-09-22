import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { mountThoughtStrip } from "../src/chat-strip.js";

const css = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");

function rule(selector) {
  return css.match(new RegExp(`\\${selector}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
}

function node() {
  const listeners = new Map();
  const classes = new Set();
  return {
    attributes: {},
    classList: {
      contains(name) {
        return classes.has(name);
      },
      toggle(name, force) {
        if (force) classes.add(name);
        else classes.delete(name);
      },
    },
    hidden: true,
    textContent: "",
    addEventListener(type, listener) {
      listeners.set(type, listener);
    },
    click() {
      listeners.get("click")();
    },
    setAttribute(name, value) {
      this.attributes[name] = value;
    },
  };
}

function thoughtRoot() {
  const root = node();
  const text = node();
  const toggle = node();
  root.querySelector = (selector) =>
    ({ ".thought-text": text, ".thought-toggle": toggle })[selector];
  return { root, text, toggle };
}

function memoryStorage() {
  const values = new Map();
  return {
    getItem(key) {
      return values.get(key) ?? null;
    },
    setItem(key, value) {
      values.set(key, value);
    },
  };
}

test("the Chat control collapses thinking and remembers that preference", () => {
  const storage = memoryStorage();
  const first = thoughtRoot();
  const strip = mountThoughtStrip(first.root, storage);

  strip.thinking("Comparing the window list with the active display.");
  assert.equal(first.root.hidden, false);
  assert.equal(first.text.textContent, "Comparing the window list with the active display.");
  assert.equal(first.toggle.textContent, "Collapse");
  assert.equal(first.toggle.attributes["aria-expanded"], "true");

  first.toggle.click();
  assert.equal(first.root.classList.contains("is-collapsed"), true);
  assert.equal(first.toggle.textContent, "Expand");
  assert.equal(first.toggle.attributes["aria-expanded"], "false");

  strip.thinking("");
  const next = thoughtRoot();
  const nextTurn = mountThoughtStrip(next.root, storage);
  nextTurn.thinking("Checking the next turn.");

  assert.equal(next.root.classList.contains("is-collapsed"), true);
  assert.equal(next.text.textContent, "Checking the next turn.");
  assert.equal(next.toggle.textContent, "Expand");
  assert.equal(next.toggle.attributes["aria-expanded"], "false");
});

test("expanded thinking reserves five lines and scrolls within them", () => {
  const expanded = rule(".thought-text");
  assert.match(expanded, /height:\s*calc\(1\.45em \* 5\)/);
  assert.match(expanded, /overflow-y:\s*auto/);
  assert.match(expanded, /white-space:\s*pre-wrap/);

  const collapsed = rule(".thought\.is-collapsed \.thought-text");
  assert.match(collapsed, /height:\s*1\.45em/);
  assert.match(collapsed, /white-space:\s*nowrap/);
  assert.match(collapsed, /text-overflow:\s*ellipsis/);
});
