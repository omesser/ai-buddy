import { spawnSync } from "node:child_process";

// The alias is under Microsoft\WindowsApps. Program Files\WindowsApps is the
// package directory, and treating that path as the alias drops a real pwsh.exe.
const APP_EXECUTION_ALIAS = /[\\/]Microsoft[\\/]WindowsApps[\\/]/i;

const MISSING = "pwsh is not installed";
const UNUSABLE = "pwsh resolved to the Windows App Execution Alias, which Node cannot spawn";

function lines(result) {
  return String(result?.stdout || "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function resolveOnWindows(spawn) {
  const found = spawn("where.exe", ["pwsh"], { encoding: "utf8", windowsHide: true });
  const foundLines = found && found.status === 0 && !found.error ? lines(found) : [];
  if (foundLines.length === 0) {
    return { kind: "missing", reason: MISSING };
  }
  const real = foundLines.find((line) => !APP_EXECUTION_ALIAS.test(line));
  if (real) {
    return { kind: "ready", command: real, args: [] };
  }
  const probe = spawn("pwsh", ["-NoProfile", "-NonInteractive", "-Command", "exit 0"], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (probe && !probe.error && probe.status === 0) {
    return { kind: "ready", command: "pwsh", args: [] };
  }
  return { kind: "unusable", reason: UNUSABLE };
}

function resolveOnPosix(spawn) {
  const found = spawn("bash", ["-lc", "command -v pwsh"], { encoding: "utf8" });
  const command = String(found?.stdout || "").trim();
  if (!found || found.status !== 0 || !command) {
    return { kind: "missing", reason: MISSING };
  }

  const probe = spawn(command, ["-NoProfile", "-NonInteractive", "-Command", "1 + 1"], {
    encoding: "utf8",
  });

  if (!probe || probe.status === null) {
    return {
      kind: "unusable",
      reason: "pwsh crashed (null exit code); Ubuntu CI still runs the real check",
    };
  }

  const output = String(probe.stdout || "") + String(probe.stderr || "");
  if (output.includes("FileLoadException") || output.includes("Abort trap")) {
    return {
      kind: "unusable",
      reason: "pwsh crashed (FileLoadException or Abort trap); Ubuntu CI still runs the real check",
    };
  }

  if (probe.status !== 0) {
    return {
      kind: "unusable",
      reason: `pwsh probe failed with exit code ${probe.status}; Ubuntu CI still runs the real check`,
    };
  }

  return { kind: "ready", command, args: [] };
}

export function resolvePwsh({ platform = process.platform, spawn = spawnSync } = {}) {
  if (platform === "win32") {
    return resolveOnWindows(spawn);
  }
  return resolveOnPosix(spawn);
}
