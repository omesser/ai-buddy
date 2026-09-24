// The README GIFs are what a dark GitHub page actually shows. The generator's
// self-check is the assertion: a near-clear white edge pixel must not survive
// as an opaque speck, and a black pupil must not be the transparency key.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SCRIPT = join(ROOT, "scripts", "generate-readme-gifs.py");

test("readme GIF generator drops the white fringe and keeps black pupils", () => {
  const out = execFileSync("python3", [SCRIPT, "--self-check"], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.match(out, /self-check:/);
});
