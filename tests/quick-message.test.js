// The quick-message composer above a Character. Timing and dismiss live here
// so a hover can be driven without a webview.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  HOVER_DELAY_MS,
  DRAG_DISMISS_PX,
  createQuickMessage,
  crossedDrag,
  placeQuickMessage,
} from "../src/quick-message.js";

function harness() {
  let time = 0;
  /** @type {{ id: number, fn: () => void, at: number }[]} */
  const tasks = [];
  let nextId = 1;
  const sent = [];
  const changes = [];
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
    onChange() {
      changes.push(qm.typing);
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

  return { qm, sent, advance, changes };
}

function shown() {
  const harnessed = harness();
  harnessed.qm.enterSprite();
  harnessed.advance(HOVER_DELAY_MS);
  return harnessed;
}

test("the composer appears after a 2.5s hover, focused, and not before", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS - 1);
  assert.equal(qm.visible, false, "a glance is not a hover");
  assert.equal(qm.typing, false);

  advance(1);
  assert.equal(qm.visible, true);
  assert.equal(qm.typing, true, "the caret is claimed as the pill appears");
  assert.equal(qm.takeFocus(), true);
  assert.equal(qm.takeFocus(), false, "the overlay focuses the field once");
});

test("leaving before the hover completes does not show the composer", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(HOVER_DELAY_MS - 1);
  qm.leaveSprite();
  advance(HOVER_DELAY_MS);
  assert.equal(qm.visible, false);
});

test("a second enter does not postpone the dwell", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(1000);
  qm.enterSprite();
  advance(HOVER_DELAY_MS - 1000);
  assert.equal(qm.visible, true);
});

test("clicking away during the dwell does not show the composer later", () => {
  const { qm, advance } = harness();

  qm.enterSprite();
  advance(1000);
  qm.outside();
  advance(HOVER_DELAY_MS);
  assert.equal(qm.visible, false);
});

test("leaving after the pill is up does not dismiss it", () => {
  const { qm, advance } = shown();

  qm.setText("hey");
  qm.leaveSprite();
  advance(500);
  assert.equal(qm.visible, true);
  assert.equal(qm.text, "hey");
  assert.equal(qm.typing, true);
});

test("blurring the field keeps the pill and releases the typing hold", () => {
  const { qm } = shown();

  qm.blur();
  assert.equal(qm.visible, true);
  assert.equal(qm.typing, false);

  qm.focus();
  assert.equal(qm.typing, true);
});

test("click outside, a pet drag, and a double-click dismiss, draft included", () => {
  for (const dismiss of ["outside", "drag", "summon"]) {
    const { qm } = shown();
    qm.setText("hey");
    qm[dismiss]();
    assert.equal(qm.visible, false, dismiss);
    assert.equal(qm.text, "", dismiss);
    assert.equal(qm.typing, false, dismiss);
  }
});

test("a poke still reaches the pet and does not dismiss the composer", () => {
  const { qm } = shown();
  let pokes = 0;
  qm.press("character", () => {
    pokes += 1;
  });
  qm.press("composer", () => {
    pokes += 1;
  });
  assert.equal(pokes, 1);
  assert.equal(qm.visible, true);
});

test("a drag is a few pixels of movement, not the click itself", () => {
  assert.equal(DRAG_DISMISS_PX, 4);
  assert.equal(crossedDrag(DRAG_DISMISS_PX - 1, 0), false);
  assert.equal(crossedDrag(DRAG_DISMISS_PX, 0), true);
  assert.equal(crossedDrag(0, DRAG_DISMISS_PX), true);
});

test("dispose clears the composer so composing cannot stick", () => {
  const { qm, changes, advance } = shown();
  qm.setText("hey");
  const before = changes.length;

  qm.dispose();
  assert.equal(qm.visible, false);
  assert.equal(qm.typing, false);
  assert.equal(qm.text, "");
  assert.equal(changes.at(-1), false);
  assert.ok(changes.length > before, "the overlay is told the hold is gone");

  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  qm.restore("hey");
  assert.equal(qm.visible, false, "dispose is final");
  assert.equal(qm.typing, false);
});

test("Escape does not dismiss", () => {
  const { qm } = shown();
  assert.equal(qm.keydown("Escape"), false);
  assert.equal(qm.visible, true);
});

test("Send delivers the trimmed line and hides the composer", () => {
  const { qm, sent } = shown();
  qm.setText("  hey, status?  ");

  assert.equal(qm.submit(), true);
  assert.deepEqual(sent, ["hey, status?"]);
  assert.equal(qm.visible, false);
  assert.equal(qm.text, "");
  assert.equal(qm.typing, false);
});

test("an empty line is not sent", () => {
  const { qm, sent } = shown();
  qm.setText("   ");

  assert.equal(qm.submit(), false);
  assert.deepEqual(sent, []);
  assert.equal(qm.visible, true);
});

test("Enter sends and Shift+Enter does not", () => {
  const { qm, sent } = shown();
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

test("the composer pops in and settles in 380ms, and leaves the same way", () => {
  const css = readFileSync(new URL("../src/main.css", import.meta.url), "utf8");

  assert.equal(HOVER_DELAY_MS, 2500);
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
  assert.match(
    css,
    /\.bubble\.quick-message \{[^}]*transition:\s*opacity 380ms cubic-bezier\(0\.16, 1, 0\.3, 1\), transform 380ms cubic-bezier\(0\.16, 1, 0\.3, 1\)/s,
    "dismiss eases out on the same 380ms the pop used",
  );
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
  assert.match(
    css,
    /\.quick-message-field \{[^}]*caret-color:\s*var\(--shared-accent\)/s,
    "the caret after autofocus is the accent from the focused still",
  );
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

test("the overlay autofocuses, dismisses on the locked gestures, and reports typing", () => {
  const js = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");

  assert.match(js, /view\.quickField\.focus\(\)/);
  assert.match(js, /invoke\("overlay_composing", \{ instance: id \}\)/);
  assert.match(js, /quickMachine\.outside\(\)/);
  assert.match(js, /quickMachine\.drag\(\)/);
  assert.match(js, /quickMachine\.summon\(\)/);
  assert.match(js, /crossedDrag\(/);
  assert.match(
    js,
    /where === "character" && event\.button === 0/,
    "only a press on the pet can become the drag that dismisses",
  );
  assert.doesNotMatch(js, /quickMachine\.keydown\("Escape"\)/);
  assert.doesNotMatch(
    js,
    /function notePointerLeft\(view\) \{\s*if \(!view\.quickMachine\.visible\) return;/,
    "a leave during the 2.5s dwell has to cancel the timer",
  );
});
