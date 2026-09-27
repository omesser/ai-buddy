// A Harness's thinking as entries in the Chat log (ADR-0034). The wire sends
// the whole thought so far and an empty one when the turn stops thinking; the
// reply landing is the other end. Each view is the entry as it should now draw.

import assert from "node:assert/strict";
import { test } from "node:test";

import { createThinking } from "../src/chat-thinking.js";

function logged() {
  const drawn = new Map();
  const draws = [];
  const thinking = createThinking((view) => {
    drawn.set(view.id, view);
    draws.push(view);
  });
  return { thinking, entries: () => [...drawn.values()], draws };
}

test("a thought streams open in one entry and the reply collapses it", () => {
  const { thinking, entries } = logged();

  thinking.thought("Reading the roster");
  thinking.thought("Reading the roster\n\nChecking the desk");
  assert.deepEqual(entries(), [
    { id: 1, text: "Reading the roster\n\nChecking the desk", state: "streaming", at: undefined },
  ]);

  thinking.landed();
  assert.deepEqual(entries(), [
    { id: 1, text: "Reading the roster\n\nChecking the desk", state: "collapsed", at: undefined },
  ]);

  thinking.toggle(1);
  assert.equal(entries()[0].state, "expanded");
  thinking.toggle(1);
  assert.equal(entries()[0].state, "collapsed");
});

test("the end of the turn collapses it, and the next turn opens its own entry", () => {
  const { thinking, entries } = logged();

  thinking.thought("Weighing a nap");
  thinking.thought("");
  thinking.landed();
  thinking.thought("Checking the desk first");

  assert.deepEqual(entries(), [
    { id: 1, text: "Weighing a nap", state: "collapsed", at: undefined },
    { id: 2, text: "Checking the desk first", state: "streaming", at: undefined },
  ]);
});

test("a reply with no thinking draws no entry", () => {
  const { thinking, draws } = logged();

  thinking.landed();
  thinking.thought("");

  assert.deepEqual(draws, []);
});

test("a block collapsed while it streams stays collapsed and keeps filling", () => {
  const { thinking, entries } = logged();

  thinking.thought("Counting the windows");
  thinking.toggle(1);
  thinking.thought("Counting the windows\nNaming the display");
  thinking.landed();

  assert.deepEqual(entries(), [
    { id: 1, text: "Counting the windows\nNaming the display", state: "collapsed", at: undefined },
  ]);
});

test("thinking kept from before the window opened arrives collapsed at its moment", () => {
  const { thinking, entries } = logged();

  thinking.kept("Reading the roster", 1_700_000_000_000);
  thinking.thought("A new turn");

  assert.deepEqual(entries(), [
    { id: 1, text: "Reading the roster", state: "collapsed", at: 1_700_000_000_000 },
    { id: 2, text: "A new turn", state: "streaming", at: undefined },
  ]);
});

test("a replaced session leaves nothing streaming", () => {
  const { thinking, entries } = logged();

  thinking.thought("Mid-thought");
  thinking.clear();
  thinking.thought("Fresh session");

  assert.deepEqual(entries().at(-1), { id: 2, text: "Fresh session", state: "streaming", at: undefined });
});
