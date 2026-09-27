import assert from "node:assert/strict";
import { test } from "node:test";

import { resolvePwsh } from "./pwsh-resolve.js";

const ALIAS = "C:\\Users\\oded\\AppData\\Local\\Microsoft\\WindowsApps\\pwsh.exe";
const REAL = "C:\\Program Files\\PowerShell\\7\\pwsh.exe";
const PACKAGE = "C:\\Program Files\\WindowsApps\\Microsoft.PowerShell_7.6.5.0_x64__8wekyb3d8bbwe\\pwsh.exe";

function spawnReturning(map) {
  return (command, args) => {
    const key = args ? command : command;
    if (Object.hasOwn(map, key)) {
      return map[key];
    }
    return { status: 1, stdout: "", stderr: "" };
  };
}

test("win32 prefers a where.exe path that is not the App Execution Alias", () => {
  const resolved = resolvePwsh({
    platform: "win32",
    spawn: spawnReturning({
      bash: { status: 0, stdout: `${ALIAS}\n`, stderr: "" },
      "where.exe": { status: 0, stdout: `${ALIAS}\r\n${REAL}\r\n`, stderr: "" },
    }),
  });
  assert.deepEqual(resolved, { kind: "ready", command: REAL, args: [] });
});

test("win32 accepts a Program Files WindowsApps package path", () => {
  const resolved = resolvePwsh({
    platform: "win32",
    spawn: spawnReturning({
      bash: { status: 0, stdout: `${ALIAS}\n`, stderr: "" },
      "where.exe": { status: 0, stdout: `${ALIAS}\r\n${PACKAGE}\r\n`, stderr: "" },
    }),
  });
  assert.deepEqual(resolved, { kind: "ready", command: PACKAGE, args: [] });
});

test("win32 spawns pwsh by name when the only match is the App Execution Alias", () => {
  const resolved = resolvePwsh({
    platform: "win32",
    spawn: spawnReturning({
      bash: { status: 0, stdout: `${ALIAS}\n`, stderr: "" },
      "where.exe": { status: 0, stdout: `${ALIAS}\r\n`, stderr: "" },
      pwsh: { status: 0, stdout: "", stderr: "" },
    }),
  });
  assert.deepEqual(resolved, { kind: "ready", command: "pwsh", args: [] });
});

test("win32 says when the App Execution Alias cannot be spawned", () => {
  const resolved = resolvePwsh({
    platform: "win32",
    spawn: spawnReturning({
      bash: { status: 0, stdout: `${ALIAS}\n`, stderr: "" },
      "where.exe": { status: 0, stdout: `${ALIAS}\r\n`, stderr: "" },
      pwsh: { status: null, error: { code: "ENOENT" }, stdout: "", stderr: "" },
    }),
  });
  assert.deepEqual(resolved, {
    kind: "unusable",
    reason: "pwsh resolved to the Windows App Execution Alias, which Node cannot spawn",
  });
});

test("win32 skips with an explicit reason when pwsh is missing", () => {
  const resolved = resolvePwsh({
    platform: "win32",
    spawn: spawnReturning({}),
  });
  assert.deepEqual(resolved, { kind: "missing", reason: "pwsh is not installed" });
});

test("non-win32 still resolves pwsh through command -v", () => {
  for (const platform of ["linux", "darwin"]) {
    const resolved = resolvePwsh({
      platform,
      spawn: spawnReturning({
        bash: { status: 0, stdout: "/usr/bin/pwsh\n", stderr: "" },
        "/usr/bin/pwsh": { status: 0, stdout: "2\n", stderr: "" },
      }),
    });
    assert.deepEqual(resolved, { kind: "ready", command: "/usr/bin/pwsh", args: [] });
  }
});

test("non-win32 detects pwsh with FileLoadException as unusable", () => {
  const resolved = resolvePwsh({
    platform: "darwin",
    spawn: spawnReturning({
      bash: { status: 0, stdout: "/usr/local/bin/pwsh\n", stderr: "" },
      "/usr/local/bin/pwsh": {
        status: 1,
        stdout: "",
        stderr: "Unhandled exception. System.IO.FileLoadException: The given assembly name was invalid.",
      },
    }),
  });
  assert.equal(resolved.kind, "unusable");
  assert(resolved.reason.includes("FileLoadException"));
});

test("non-win32 detects pwsh with Abort trap as unusable", () => {
  const resolved = resolvePwsh({
    platform: "linux",
    spawn: spawnReturning({
      bash: { status: 0, stdout: "/usr/bin/pwsh\n", stderr: "" },
      "/usr/bin/pwsh": { status: 134, stdout: "", stderr: "Abort trap: 6" },
    }),
  });
  assert.equal(resolved.kind, "unusable");
  assert(resolved.reason.includes("Abort trap"));
});

test("non-win32 detects pwsh with null exit code as unusable", () => {
  const resolved = resolvePwsh({
    platform: "darwin",
    spawn: spawnReturning({
      bash: { status: 0, stdout: "/usr/local/bin/pwsh\n", stderr: "" },
      "/usr/local/bin/pwsh": { status: null, stdout: "", stderr: "" },
    }),
  });
  assert.equal(resolved.kind, "unusable");
  assert(resolved.reason.includes("null exit code"));
});

test("non-win32 detects pwsh probe failure as unusable", () => {
  const resolved = resolvePwsh({
    platform: "linux",
    spawn: spawnReturning({
      bash: { status: 0, stdout: "/usr/bin/pwsh\n", stderr: "" },
      "/usr/bin/pwsh": { status: 127, stdout: "", stderr: "Command not found" },
    }),
  });
  assert.equal(resolved.kind, "unusable");
  assert(resolved.reason.includes("exit code 127"));
});

test("non-win32 skips with an explicit reason when pwsh is missing", () => {
  const resolved = resolvePwsh({
    platform: "linux",
    spawn: spawnReturning({}),
  });
  assert.deepEqual(resolved, { kind: "missing", reason: "pwsh is not installed" });
});
