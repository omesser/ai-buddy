// Run with `node --test tests/`.
// The hook shells out to pwsh. These cases stub it on PATH so a Darwin
// Abort trap can be proven on Linux, where the crash does not reproduce.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

const hook = join(
  import.meta.dirname,
  "..",
  "scripts",
  "hooks",
  "psscriptanalyzer.sh",
);

const CRASH_OUTPUT =
  "Unhandled exception. System.IO.FileLoadException: The given assembly name was invalid.";

const stubPwsh = `#!/usr/bin/env bash
case "$*" in
  *Get-Module*) exit "\${PSSA_MODULE_STATUS:-0}" ;;
esac
if [ -n "\${PSSA_OUTPUT:-}" ]; then
  printf '%s\\n' "\$PSSA_OUTPUT"
fi
exit "\${PSSA_STATUS:-0}"
`;

const runHook = ({ env = {}, pathHasPwsh = true } = {}) => {
  const tmpDir = mkdtempSync(join(tmpdir(), "pssa-hook-"));
  const bin = join(tmpDir, "bin");
  mkdirSync(bin);
  if (pathHasPwsh) {
    const pwsh = join(bin, "pwsh");
    writeFileSync(pwsh, stubPwsh);
    chmodSync(pwsh, 0o755);
  }
  writeFileSync(join(tmpDir, "probe.ps1"), "Write-Host 'hi'\n");

  // ubuntu-latest ships pwsh in /usr/bin. A missing-pwsh case must not
  // inherit that PATH or the hook finds the real binary and never skips.
  // Invoke via /bin/bash so the hook does not need bash on PATH.
  const path = pathHasPwsh ? `${bin}:${process.env.PATH ?? ""}` : bin;

  const result = spawnSync("/bin/bash", [hook, "probe.ps1"], {
    cwd: tmpDir,
    encoding: "utf8",
    env: { ...process.env, ...env, PATH: path },
  });
  result.bin = bin;
  return result;
};

test("missing pwsh skips with the existing message", () => {
  const result = runHook({ pathHasPwsh: false });
  assert.equal(existsSync(join(result.bin, "pwsh")), false);
  assert.equal(result.status, 0);
  assert.match(result.stderr, /skipped - pwsh not installed/);
});

test("missing PSScriptAnalyzer skips with the existing message", () => {
  const result = runHook({ env: { PSSA_MODULE_STATUS: "1" } });
  assert.equal(result.status, 0);
  assert.match(result.stderr, /skipped - PSScriptAnalyzer not installed/);
});

test("a FileLoadException Abort trap skips instead of failing", () => {
  const result = runHook({
    env: { PSSA_STATUS: "134", PSSA_OUTPUT: CRASH_OUTPUT },
  });
  assert.equal(result.status, 0);
  assert.match(result.stderr, /skipped - pwsh crashed running PSScriptAnalyzer/);
  assert.match(result.stderr, /FileLoadException/);
});

test("Abort trap text with exit 134 skips", () => {
  const result = runHook({
    env: { PSSA_STATUS: "134", PSSA_OUTPUT: "Abort trap: 6" },
  });
  assert.equal(result.status, 0);
  assert.match(result.stderr, /skipped - pwsh crashed running PSScriptAnalyzer/);
});

test("exit 134 with no analyzer finding text skips", () => {
  const result = runHook({ env: { PSSA_STATUS: "134" } });
  assert.equal(result.status, 0);
  assert.match(result.stderr, /skipped - pwsh crashed running PSScriptAnalyzer/);
});

test("a ParseError finding still fails the hook", () => {
  const result = runHook({
    env: { PSSA_STATUS: "1", PSSA_OUTPUT: "ParseError UnexpectedToken" },
  });
  assert.equal(result.status, 1);
  assert.match(result.stdout, /ParseError UnexpectedToken/);
  assert.doesNotMatch(result.stderr, /skipped - pwsh crashed/);
});

test("an Error finding still fails the hook", () => {
  const result = runHook({
    env: {
      PSSA_STATUS: "1",
      PSSA_OUTPUT: "Severity Error RuleName PSAvoidUsingCmdletAliases",
    },
  });
  assert.equal(result.status, 1);
  assert.match(result.stdout, /Severity Error/);
  assert.doesNotMatch(result.stderr, /skipped - pwsh crashed/);
});

test("exit 134 with Severity findings is not treated as a crash", () => {
  const result = runHook({
    env: {
      PSSA_STATUS: "134",
      PSSA_OUTPUT: "Severity Error RuleName PSAvoidUsingCmdletAliases",
    },
  });
  assert.equal(result.status, 134);
  assert.match(result.stdout, /Severity Error/);
  assert.doesNotMatch(result.stderr, /skipped - pwsh crashed/);
});

test("a clean analyzer run exits 0", () => {
  const result = runHook();
  assert.equal(result.status, 0);
  assert.equal(result.stderr, "");
});
