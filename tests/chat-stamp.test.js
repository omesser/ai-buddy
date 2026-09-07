// Run with `node --test tests/`.
//
// The Chat surface's turn stamps: local clock on a line, and when a later
// line needs the calendar day said out loud.

import assert from "node:assert/strict";
import { test } from "node:test";

import { stampWhen } from "../src/chat-stamp.js";

test("the first turn stamps local HH:mm", () => {
  const at = new Date(2026, 8, 7, 14, 32, 7);
  const stamp = stampWhen(at, null);

  assert.equal(stamp.label, "14:32");
});

test("a later turn on the same local day stays HH:mm", () => {
  const previousAt = new Date(2026, 8, 7, 14, 32, 7);
  const at = new Date(2026, 8, 7, 18, 5, 0);
  const stamp = stampWhen(at, previousAt);

  assert.equal(stamp.label, "18:05");
});

test("a turn after midnight prefixes the new local day", () => {
  const previousAt = new Date(2026, 8, 7, 23, 50, 0);
  const at = new Date(2026, 8, 8, 0, 5, 0);
  const stamp = stampWhen(at, previousAt);

  assert.equal(stamp.label, "8 Sep 00:05");
});

test("a later turn on the new local day goes back to HH:mm", () => {
  const previousAt = new Date(2026, 8, 8, 0, 5, 0);
  const at = new Date(2026, 8, 8, 9, 12, 0);
  const stamp = stampWhen(at, previousAt);

  assert.equal(stamp.label, "09:12");
});

test("a stamp's datetime is the instant as ISO-8601", () => {
  const at = new Date(2026, 8, 7, 14, 32, 7);
  const stamp = stampWhen(at, null);

  assert.match(stamp.datetime, /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:07\.000Z$/);
  assert.equal(new Date(stamp.datetime).getTime(), at.getTime());
});

test("hover title is the local day, year, and clock with seconds", () => {
  const at = new Date(2026, 8, 7, 14, 32, 7);
  const stamp = stampWhen(at, null);

  assert.equal(stamp.title, "7 Sep 2026, 14:32:07");
});

test("the year lives on the hover title, not the label", () => {
  const previousAt = new Date(2025, 11, 31, 23, 50, 0);
  const at = new Date(2026, 0, 1, 0, 5, 3);
  const stamp = stampWhen(at, previousAt);

  assert.equal(stamp.label, "1 Jan 00:05");
  assert.equal(stamp.title, "1 Jan 2026, 00:05:03");
});
