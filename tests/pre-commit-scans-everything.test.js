// `detect-private-key`, `detect-aws-credentials` and `check-added-large-files`
// read every tracked file, vendored content included: bytes a bot refreshes
// weekly are what a secret scanner is for. Every other hook skips the vendored
// tree by alias, so an exclude is one keystroke away; this test forbids it.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const config = readFileSync(
  fileURLToPath(new URL("../.pre-commit-config.yaml", import.meta.url)),
  "utf8",
);
const lines = config.split("\n");

const SCANS_EVERY_FILE = [
  "detect-private-key",
  "detect-aws-credentials",
  "check-added-large-files",
];

// Anything that would hand the hook a shorter file list. `args` is absent on
// purpose: it tunes what a hook reports, not which files reach it.
const NARROWS = /^\s*(exclude|files|types|types_or|exclude_types)\s*:/;

// A hook's own keys are the lines after its `- id:` up to the next list item at
// the same or shallower indent -- the next hook, or the next repo entry.
const hookBody = (id) => {
  const marker = `- id: ${id}`;
  const starts = lines.flatMap((l, i) => (l.trim() === marker ? [i] : []));
  assert.equal(
    starts.length,
    1,
    `expected exactly one \`${marker}\` in .pre-commit-config.yaml, found ${starts.length}`,
  );

  const start = starts[0];
  const indent = lines[start].indexOf("-");
  const body = [];
  for (const line of lines.slice(start + 1)) {
    if (line.trim() === "") continue;
    const at = line.search(/\S/);
    if (at < indent || (at === indent && line.trim().startsWith("-"))) break;
    body.push(line);
  }
  return body;
};

for (const id of SCANS_EVERY_FILE) {
  test(`${id} narrows its file list with nothing`, () => {
    const narrowing = hookBody(id).filter((l) => NARROWS.test(l));
    assert.deepEqual(
      narrowing,
      [],
      `${id} must see every file, the vendored tree included. Remove:\n${narrowing.join("\n")}`,
    );
  });
}

// The guard above only means something while the file still reads the way it
// parses here. If the vendored exclude ever stops being shared by alias, the
// paste it replaced is back and these three are no longer the odd ones out.
test("the vendored exclude is defined once and shared by alias", () => {
  const anchors = lines.filter((l) => /^\s*exclude:\s*&vendored\s/.test(l));
  const aliases = lines.filter((l) => /^\s*exclude:\s*\*vendored\s*$/.test(l));

  assert.equal(anchors.length, 1, "expected exactly one `exclude: &vendored` definition");
  assert.equal(aliases.length, 6, `expected 6 \`exclude: *vendored\` aliases, found ${aliases.length}`);
});
