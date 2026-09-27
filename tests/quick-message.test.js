// The quick-message composer above a Character. Timing and dismiss live here
// so a hover can be driven without a webview.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  HOVER_DELAY_MS,
  LEAVE_GRACE_MS,
  createQuickMessage,
  placeQuickMessage,
} from "../src/quick-message.js";

function harness() {
  let time = 0;
  /** @type {{ id: number, fn: () => void, at: number }[]} */
  const tasks = [];
  let nextId = 1;
  const sent = [];
  const qm = createQuickMessage({
    schedule(fn, ms) {
      const id = nextId;
      nextId += 1;
      tasks.push({ id, fn, at: time + ms });
      return id;
    },
    clear(id) {
      const index = tasks.findIndex((task) => task.id === id);
      if (index >= 0) tasks.splice(index, 1);
    },
    send(text) {
      sent.push(text);
    },
  });

  function advance(ms) {
    time += ms;
    for (let guard = 0; guard < 20; guard += 1) {
      const due = tasks
        .filter((task) => task.at <= time)
        .sort((a, b) => a.at - b.at || a.id - b.id);
      if (due.length === 0) return;
      const task = due[0];
      const index = tasks.indexOf(task);
      tasks.splice(index, 1);
      task.fn();
    }
  }

  return { qm, sent, advance };
}

test("the composer appears after a 200ms hover and not before", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS - 1);
  assert.equal(qm.visible, false, "a glance is not a hover");

  advance(1);
  assert.equal(qm.visible, true);
});

test("leaving before the hover completes does not show the composer", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS - 1);
  qm.leaveSprite();
  advance(HOVER_DELAY_MS);
  assert.equal(qm.visible, false);
});

test("leaving the character hides the composer once the gap is crossed", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.leaveSprite();
  advance(LEAVE_GRACE_MS - 1);
  assert.equal(qm.visible, true, "the pointer is still crossing onto the composer");

  advance(1);
  assert.equal(qm.visible, false);
});

test("moving onto the composer during the leave grace keeps it", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.leaveSprite();
  advance(LEAVE_GRACE_MS - 1);
  qm.enterComposer();
  advance(LEAVE_GRACE_MS);
  assert.equal(qm.visible, true);
});

test("a draft keeps the composer up after the pointer leaves", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.setText("hey, status?");
  qm.leaveSprite();
  advance(LEAVE_GRACE_MS + 50);
  assert.equal(qm.visible, true);
  assert.equal(qm.text, "hey, status?");
});

test("a focused empty field stays up when the pointer leaves", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.focus();
  qm.leaveSprite();
  qm.leaveComposer();
  advance(LEAVE_GRACE_MS + 50);
  assert.equal(qm.visible, true);
});

test("Escape dismisses an empty composer and keeps a draft", () => {
  const empty = harness();
  empty.qm.enterSprite();
  empty.advance(HOVER_DELAY_MS);
  assert.equal(empty.qm.keydown("Escape"), true);
  assert.equal(empty.qm.visible, false);

  const draft = harness();
  draft.qm.enterSprite();
  draft.advance(HOVER_DELAY_MS);
  draft.qm.setText("hey");
  assert.equal(draft.qm.keydown("Escape"), false);
  assert.equal(draft.qm.visible, true);
  assert.equal(draft.qm.text, "hey");
});

test("a press on the character still reaches the pet", () => {
  const { qm, advance } = harness();
  qm.enterSprite();
  advance(HOVER_DELAY_MS);

  let pokes = 0;
  qm.press("character", () => {
    pokes += 1;
  });
  qm.press("composer", () => {
    pokes += 1;
  });
  assert.equal(pokes, 1);
});

test("Send delivers the trimmed line and hides the composer", () => {
  const { qm, sent, advance } = harness();
  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.setText("  hey, status?  ");

  assert.equal(qm.submit(), true);
  assert.deepEqual(sent, ["hey, status?"]);
  assert.equal(qm.visible, false);
  assert.equal(qm.text, "");
});

test("an empty line is not sent", () => {
  const { qm, sent, advance } = harness();
  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.setText("   ");

  assert.equal(qm.submit(), false);
  assert.deepEqual(sent, []);
  assert.equal(qm.visible, true);
});

test("Enter sends and Shift+Enter does not", () => {
  const { qm, sent, advance } = harness();
  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.setText("hey");

  assert.equal(qm.keydown("Enter", { shiftKey: true }), false);
  assert.deepEqual(sent, []);
  assert.equal(qm.visible, true);

  assert.equal(qm.keydown("Enter"), true);
  assert.deepEqual(sent, ["hey"]);
  assert.equal(qm.visible, false);
});

test("the composer sits above Speech when the two would share a box", () => {
  const sprite = { x: 100, y: 400, width: 64, height: 64 };
  const size = { width: 200, height: 40 };
  const bounds = { x: 0, y: 0, width: 1000, height: 800 };
  const speech = { x: 32, y: 350, width: 200, height: 80 };

  const clear = placeQuickMessage(sprite, size, bounds, null);
  assert.equal(clear.y, 350, "same 10px gap placeBubble leaves over the head");

  const stacked = placeQuickMessage(sprite, size, bounds, speech);
  assert.equal(stacked.y, 300, "one gap above the Speech bubble, not on top of it");
});

test("reporting the leave again does not postpone hiding", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.leaveSprite();
  advance(LEAVE_GRACE_MS - 1);
  qm.leaveSprite();
  advance(1);
  assert.equal(qm.visible, false);
});

test("the composer pops in and settles in 380ms, still without a tail", () => {
  const css = readFileSync(new URL("../src/main.css", import.meta.url), "utf8");

  assert.equal(HOVER_DELAY_MS, 200, "the dwell before the pop stays a hover, not a glance");
  assert.match(
    css,
    /\.bubble\.quick-message::before,\s*\.bubble\.quick-message::after\s*\{[^}]*content:\s*none/s,
  );
  assert.match(
    css,
    /animation:\s*quick-message-pop 380ms linear/,
    "the clock is linear so the overshoot is not spent in the first moments",
  );
  assert.match(
    css,
    /@keyframes quick-message-pop[\s\S]*cubic-bezier\(0\.16, 1, 0\.3, 1\)[\s\S]*translateY\(8px\) scale\(0\.94\)[\s\S]*translateY\(-2px\) scale\(1\.02\)[\s\S]*translateY\(0\) scale\(1\)/,
    "the pop eases out through 1.02, rises, and settles at 1",
  );
  assert.doesNotMatch(css, /scale\(1\.03\)/);
  assert.match(css, /transform-origin:\s*center bottom/);
});

test("the empty composer says talk to me and Send is a triangle, not a link", () => {
  const js = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const css = readFileSync(new URL("../src/main.css", import.meta.url), "utf8");

  assert.match(js, /placeholder = "talk to me"/);
  assert.match(js, /className = "quick-message-send"/);
  assert.match(js, /setAttribute\("aria-label", "Send"\)/);
  assert.match(js, /M2\.2 1\.4 L10\.2 6 L2\.2 10\.6/, "Send is the dark triangle from the motion mock");
  assert.match(js, /fill", "#14171e"/);
  assert.doesNotMatch(js, /send\.textContent = "Send"/);
  assert.doesNotMatch(js, /send\.className = "bubble-more"/);
  assert.match(css, /\.quick-message-send \{[^}]*width:\s*28px;\s*height:\s*28px;[^}]*border-radius:\s*50%;[^}]*background:\s*var\(--shared-accent\)/s);
  assert.match(css, /\.quick-message-field::placeholder \{[^}]*color:\s*#8b93a7/s);
  assert.match(css, /\.quick-message-send svg[\s\S]*fill:\s*#14171e/);
  assert.doesNotMatch(
    css,
    /\.bubble\.quick-message\.visible \.bubble-more/,
    "the composer does not borrow the Open chat underline",
  );
});

test("overlay Send uses chat_send and does not poke the pet", () => {
  const js = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  assert.match(
    js,
    /invoke\("chat_send", \{ instance: id, text, echo: true \}\)/,
    "the line joins the Chat pipeline, and an open Chat surface is told",
  );
  assert.match(js, /quickMachine\.press\(where/, "a composer press is not a Poke");
});
