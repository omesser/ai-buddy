// The quick-message composer above a Character. Timing and dismiss live here
// so a hover can be driven without a webview.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  HOVER_DELAY_MS,
  DRAG_DISMISS_PX,
  applyQuickMessageGate,
  createQuickMessage,
  crossedDrag,
  placeQuickMessage,
  quickMessageConnects,
  quickMessageMirror,
  quickMessagePrompt,
} from "../src/quick-message.js";

function harness(options = {}) {
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
    available: options.available,
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

test("leaving after the pill is up does not dismiss it immediately", () => {
  const { qm, advance } = shown();

  qm.setText("hey");
  qm.leaveSprite();
  advance(500);
  assert.equal(qm.visible, true);
  assert.equal(qm.text, "hey");
  assert.equal(qm.typing, true);
});

test("pill auto-hides after 3s if empty and pointer leaves both sprite and pill", () => {
  const { qm, advance } = shown();

  // No text entered, leave sprite
  qm.leaveSprite();
  // Also leave pill
  qm.leavePill();

  // After 2.9s, still visible
  advance(2900);
  assert.equal(qm.visible, true, "pill stays visible before 3s");

  // After 3s total, hides
  advance(100);
  assert.equal(qm.visible, false, "pill auto-hides after 3s continuous away");
});

test("auto-hide cancels if pointer re-enters sprite before 3s", () => {
  const { qm, advance } = shown();

  qm.leaveSprite();
  qm.leavePill();
  advance(2000);

  // Re-enter sprite before 3s
  qm.enterSprite();
  advance(2000);

  assert.equal(qm.visible, true, "pill stays visible when sprite re-entered");
});

test("re-entering sprite resets auto-hide timer to fresh 3s on next leave", () => {
  const { qm, advance } = shown();

  // Leave both, wait 2s (partial)
  qm.leaveSprite();
  qm.leavePill();
  advance(2000);

  // Re-enter sprite (cancels timer)
  qm.enterSprite();

  // Leave again - should start fresh 3s, not continue from 2s
  qm.leaveSprite();
  advance(2900);
  assert.equal(qm.visible, true, "pill still visible at 2.9s of fresh timer");

  advance(100);
  assert.equal(qm.visible, false, "pill hides after fresh 3s from second leave");
});

test("auto-hide cancels if pointer re-enters pill before 3s", () => {
  const { qm, advance } = shown();

  qm.leaveSprite();
  qm.leavePill();
  advance(2000);

  // Re-enter pill before 3s
  qm.enterPill();
  advance(2000);

  assert.equal(qm.visible, true, "pill stays visible when pill re-entered");
});

test("auto-hide does not trigger if user has typed non-empty text", () => {
  const { qm, advance } = shown();

  qm.setText("hey");
  qm.leaveSprite();
  qm.leavePill();

  // Even after 3s, should not auto-hide because text is present
  advance(3000);
  assert.equal(qm.visible, true, "pill with text does not auto-hide");
  assert.equal(qm.text, "hey");
});

test("auto-hide treats whitespace-only as empty", () => {
  const { qm, advance } = shown();

  qm.setText("   ");
  qm.leaveSprite();
  qm.leavePill();

  advance(3000);
  assert.equal(qm.visible, false, "pill with only whitespace auto-hides");
});

test("auto-hide does not trigger if only sprite is left but pill is hovered", () => {
  const { qm, advance } = shown();

  // Simulate entering the pill (user moved from sprite to pill)
  qm.enterPill();
  qm.leaveSprite();

  advance(3000);
  assert.equal(qm.visible, true, "pill stays visible while hovered");
});

test("auto-hide does not trigger if only pill is left but sprite is hovered", () => {
  const { qm, advance } = shown();

  // Leave pill but not sprite (shouldn't happen in practice but test the logic)
  qm.leavePill();
  // Still on sprite

  advance(3000);
  assert.equal(qm.visible, true, "pill stays visible while sprite hovered");
});

test("existing dismiss paths still work with auto-hide feature", () => {
  const { qm } = shown();

  // outside dismiss
  qm.outside();
  assert.equal(qm.visible, false);

  // Show again and test drag dismiss
  const { qm: qm2 } = shown();
  qm2.drag();
  assert.equal(qm2.visible, false);

  // Show again and test summon dismiss
  const { qm: qm3 } = shown();
  qm3.summon();
  assert.equal(qm3.visible, false);
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

test("an unavailable pill appears but refuses text, focus, and send", () => {
  const { qm, sent, advance } = harness({ available: false });

  qm.enterSprite();
  advance(HOVER_DELAY_MS);
  assert.equal(qm.visible, true, "the status still has to be readable");
  assert.equal(qm.available, false);
  assert.equal(qm.typing, false, "a field that takes nothing is not a typing hold");
  assert.equal(qm.takeFocus(), false);

  qm.setText("hey");
  qm.focus();
  assert.equal(qm.text, "");
  assert.equal(qm.typing, false);
  assert.equal(qm.keydown("Enter"), false);
  assert.equal(qm.submit(), false);
  assert.deepEqual(sent, []);
  assert.equal(qm.visible, true);
});

test("a refused send brings the frozen pill back without the line", () => {
  const { qm, sent } = shown();
  qm.setText("hey");
  qm.dismiss();
  qm.setAvailable(false);

  qm.restore("hey");
  assert.equal(qm.visible, true);
  assert.equal(qm.text, "");
  assert.equal(qm.typing, false);
  assert.deepEqual(sent, []);
});

test("freezing clears a draft and thawing claims the caret", () => {
  const { qm, sent } = shown();
  qm.setText("hey");

  qm.setAvailable(false);
  assert.equal(qm.visible, true);
  assert.equal(qm.text, "", "a draft would hide the unavailable sentence");
  assert.equal(qm.typing, false);
  assert.equal(qm.submit(), false);
  assert.deepEqual(sent, []);

  qm.setAvailable(true);
  assert.equal(qm.available, true);
  assert.equal(qm.typing, true);
  assert.equal(qm.takeFocus(), true);
  qm.setText("hey");
  assert.equal(qm.submit(), true);
  assert.deepEqual(sent, ["hey"]);
});

test("the unavailable pill points at Chat instead of echoing its composer", () => {
  const off = { name: "bmo", configured: true, enabled: false };
  assert.equal(quickMessagePrompt(off), "Connect an AI to talk to me");
  assert.equal(quickMessagePrompt(null), "Connect an AI to talk to me");
  assert.equal(quickMessageConnects(off), true);
  assert.equal(quickMessageConnects(null), true);
  const login = { name: "bmo", configured: true, enabled: true, login: "claude /login" };
  assert.equal(quickMessagePrompt(login), "Connect an AI to talk to me");
  assert.equal(quickMessageConnects(login), true);

  const starting = {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "hermes",
    harness: { name: "hermes", alive: false, initializing: true },
  };
  assert.equal(quickMessagePrompt(starting), "Starting Hermes…");
  assert.equal(quickMessageConnects(starting), false, "a wait that ends on its own is not a link");

  const down = {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "hermes",
    harness: { name: "hermes", alive: false, session: null },
  };
  assert.equal(quickMessagePrompt(down), "Connect an AI to talk to me");
  assert.equal(quickMessageConnects(down), true);

  const ready = { name: "bmo", configured: true, enabled: true };
  assert.equal(quickMessagePrompt(ready), "talk to me");
  assert.equal(quickMessageConnects(ready), false);
  assert.equal(
    quickMessageMirror("", "Connect an AI to talk to me", false),
    "Connect an AI to talk to me\u200b",
    "the frozen pill reserves the sentence, or overflow clips it",
  );
  assert.equal(quickMessageMirror("", "talk to me", true), "\u200b");
  assert.equal(quickMessageMirror("hey", "talk to me", true), "hey\u200b");
});

function gateDouble() {
  return {
    field: {
      disabled: false,
      placeholder: "",
      labels: {},
      setAttribute(name, value) {
        this.labels[name] = value;
      },
    },
    send: { disabled: false, hidden: false },
    link: { hidden: true },
    machine: {
      ready: true,
      setAvailable(value) {
        this.ready = value;
      },
    },
  };
}

test("a draft does not hide the unavailable sentence when the pill freezes", () => {
  const { qm } = shown();
  qm.setText("still drafting");
  const gate = gateDouble();
  gate.machine = qm;

  applyQuickMessageGate(gate, { name: "bmo", configured: true, enabled: false });

  assert.equal(qm.text, "");
  assert.equal(gate.field.placeholder, "Connect an AI to talk to me");
  assert.equal(
    quickMessageMirror(qm.text, gate.field.placeholder, qm.available),
    "Connect an AI to talk to me\u200b",
    "leftover draft text would hide the sentence the placeholder is showing",
  );
});

test("the gate disables the field and send from the same opening chat uses", () => {
  const frozen = gateDouble();
  applyQuickMessageGate(frozen, { name: "bmo", configured: true, enabled: false });
  assert.equal(frozen.machine.ready, false);
  assert.equal(frozen.field.disabled, true);
  assert.equal(frozen.send.disabled, true);
  assert.equal(frozen.field.placeholder, "Connect an AI to talk to me");
  assert.equal(frozen.field.labels["aria-label"], "Connect an AI to talk to me");
  assert.equal(frozen.link.hidden, false, "the hint carries a link that opens Chat");
  assert.equal(frozen.send.hidden, true, "Send leaves so the link is the pill's one control");

  const starting = gateDouble();
  applyQuickMessageGate(starting, {
    name: "bmo",
    configured: true,
    enabled: true,
    harness_name: "cursor-agent",
    harness: { name: "cursor-agent", alive: false, initializing: true },
  });
  assert.equal(starting.field.disabled, true);
  assert.equal(starting.send.disabled, true);
  assert.equal(starting.field.placeholder, "Starting Cursor…");
  assert.equal(starting.link.hidden, true);
  assert.equal(starting.send.hidden, false);

  const ready = gateDouble();
  applyQuickMessageGate(ready, { name: "bmo", configured: true, enabled: true });
  assert.equal(ready.machine.ready, true);
  assert.equal(ready.field.disabled, false);
  assert.equal(ready.send.disabled, false);
  assert.equal(ready.field.placeholder, "talk to me");
  assert.equal(ready.field.labels["aria-label"], "Quick message");
  assert.equal(ready.link.hidden, true);
  assert.equal(ready.send.hidden, false);
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

  assert.equal(
    quickMessagePrompt({ name: "bmo", configured: true, enabled: true }),
    "talk to me",
  );
  assert.match(js, /paintQuickGate\(view, null\)/);
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

test("the overlay freezes the pill from chat's opening, including while it is already up", () => {
  const js = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const rs = readFileSync(new URL("../src-tauri/src/main.rs", import.meta.url), "utf8");

  assert.match(js, /applyQuickMessageGate\(/);
  assert.match(
    js,
    /listen\(\s*"chat-opening",[\s\S]*?target: overlay\.label/,
    "the same push that thaws Chat reaches this overlay",
  );
  assert.match(js, /invoke\("chat_opening", \{ instance: id \}\)/);
  assert.match(rs, /fan_out_chat_opening\(/);
  assert.match(
    rs,
    /emit_to\(target, event, &opening\)/,
    "emit_to a Chat label does not reach the overlay, and Chat may not be open",
  );
});

test("a frozen send control does not keep the accent disc", () => {
  const css = readFileSync(new URL("../src/main.css", import.meta.url), "utf8");
  assert.match(
    css,
    /\.quick-message-send:disabled \{[^}]*background:\s*#2c3340;[^}]*cursor:\s*default/,
  );
  assert.doesNotMatch(
    css,
    /\.quick-message-send:disabled \{[^}]*var\(--shared-accent\)/,
  );
  assert.match(css, /\.quick-message-field:disabled \{[^}]*cursor:\s*default/);
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
  assert.match(
    js,
    /function removeView\(id\) \{[\s\S]*?quickMachine\.dispose\(\)[\s\S]*?views\.delete\(id\);[\s\S]*?reportComposing\(\);/,
    "dispose leaves the map, then the composing report cannot keep that id",
  );
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

test("without a clickable link the pill still names the fix as text", () => {
  const gate = gateDouble();
  gate.link = null;
  applyQuickMessageGate(gate, { name: "bmo", configured: false, enabled: false });
  assert.equal(gate.field.placeholder, "Connect an AI to talk to me");
  assert.equal(gate.field.disabled, true);
  assert.equal(gate.send.hidden, false, "with no link to stand in, Send keeps its place");
});
