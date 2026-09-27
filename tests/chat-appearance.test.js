import assert from "node:assert/strict";
import { test } from "node:test";
import { mountChatAppearance } from "../src/chat-appearance.js";

class FakeMedia {
  constructor(matches) {
    this.matches = matches;
    this.listeners = [];
  }

  addEventListener(type, fn) {
    if (type === "change") {
      this.listeners.push(fn);
    }
  }

  fire(matches) {
    this.matches = matches;
    const event = { matches };
    for (const fn of this.listeners) {
      fn(event);
    }
  }
}

function root(className) {
  const classes = className.split(/\s+/).filter(Boolean);
  return {
    dataset: {},
    classList: {
      add(name) {
        if (!classes.includes(name)) {
          classes.push(name);
        }
      },
      remove(...names) {
        for (const name of names) {
          const at = classes.indexOf(name);
          if (at >= 0) {
            classes.splice(at, 1);
          }
        }
      },
      contains(name) {
        return classes.includes(name);
      },
      toString() {
        return classes.join(" ");
      },
    },
  };
}

test("forced light stays light when the OS flips to dark", () => {
  const media = new FakeMedia(true);
  const el = root("chat-ui-minimal");
  const apply = mountChatAppearance(el, media);
  apply("light");
  media.fire(false);
  assert.equal(el.dataset.chatPalette, "light");
});

test("forced dark stays dark when the OS flips to light", () => {
  const media = new FakeMedia(false);
  const el = root("chat-ui-minimal");
  const apply = mountChatAppearance(el, media);
  apply("dark");
  media.fire(true);
  assert.equal(el.dataset.chatPalette, "dark");
});

test("system follows the OS, including after a forced mode", () => {
  const media = new FakeMedia(true);
  const el = root("chat-ui-minimal");
  const apply = mountChatAppearance(el, media);
  assert.equal(el.dataset.chatPalette, "light");
  media.fire(false);
  assert.equal(el.dataset.chatPalette, "dark");
  apply("light");
  assert.equal(el.dataset.chatPalette, "light");
  apply("system");
  assert.equal(el.dataset.chatPalette, "dark");
  media.fire(true);
  assert.equal(el.dataset.chatPalette, "light");
});

test("unknown wire values become the OS palette", () => {
  const media = new FakeMedia(true);
  const el = root("chat-ui-minimal");
  const apply = mountChatAppearance(el, media);
  apply("dark");
  for (const wire of ["sepia", "System", "Light", "DARK", null, undefined, 1, ""]) {
    apply(wire);
    assert.equal(el.dataset.chatPalette, "light", String(wire));
  }
});

test("apply does not change the Chat UI class", () => {
  const media = new FakeMedia(false);
  const el = root("chat-ui-terminal");
  const apply = mountChatAppearance(el, media);
  apply("light");
  apply("dark");
  apply("system");
  assert.equal(el.classList.toString(), "chat-ui-terminal");
});

test("a second apply of the same mode keeps data-chat-palette", () => {
  const media = new FakeMedia(false);
  const el = root("chat-ui-minimal");
  const apply = mountChatAppearance(el, media);
  apply("light");
  assert.equal(el.dataset.chatPalette, "light");
  apply("light");
  assert.equal(el.dataset.chatPalette, "light");
});
