import { spawnSync } from "node:child_process";

export function resolvePwsh({ spawn = spawnSync } = {}) {
  const found = spawn("bash", ["-lc", "command -v pwsh"], { encoding: "utf8" });
  const command = String(found?.stdout || "").trim();
  if (found && found.status === 0 && command) {
    return { kind: "ready", command, args: [] };
  }
  return { kind: "missing", reason: "pwsh is not installed" };
}
