// Run with `node --test tests/`.
// Each case is a throwaway git repo with the probe file staged.
// That is the same surface pre-commit uses. #804.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

const hook = join(
  import.meta.dirname,
  "..",
  "scripts",
  "hooks",
  "check-comment-blocks.py",
);

const FOUR_LINE_BLOCK = `// one
// two
// three
// four
`;

const THREE_LINE_BLOCK = `// one
// two
// three
`;

const gitEnv = () => {
  const env = { ...process.env };
  delete env.GIT_DIR;
  delete env.GIT_WORK_TREE;
  return env;
};

const runHook = (files) => {
  const tmpDir = mkdtempSync(join(tmpdir(), "comment-blocks-"));
  const env = gitEnv();
  const git = (args) =>
    spawnSync("git", args, { cwd: tmpDir, encoding: "utf8", env });

  git(["init", "-q"]);
  git(["config", "user.email", "test@example.com"]);
  git(["config", "user.name", "test"]);

  for (const [path, content] of Object.entries(files)) {
    writeFileSync(join(tmpDir, path), content);
    git(["add", "--", path]);
  }

  return spawnSync("python3", [hook, ...Object.keys(files)], {
    cwd: tmpDir,
    encoding: "utf8",
    env,
  });
};

test("a newly added 4-line Swift // block fails the hook", () => {
  const result = runHook({
    "probe.swift": `${FOUR_LINE_BLOCK}print("ok")\n`,
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /probe\.swift/);
  assert.match(result.stderr, /4-line block/);
});

test("a newly added 3-line Swift // block passes the hook", () => {
  const result = runHook({
    "probe.swift": `${THREE_LINE_BLOCK}print("ok")\n`,
  });

  assert.equal(result.status, 0);
  assert.equal(result.stderr, "");
});

test("a newly added 4-line Rust // block still fails the hook", () => {
  const result = runHook({
    "probe.rs": `${FOUR_LINE_BLOCK}fn main() {}\n`,
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /probe\.rs/);
  assert.match(result.stderr, /4-line block/);
});
