// scripts/verify-anchor-offscreen-win.ps1 decides "anchor pixels on the desktop"
// in Test-AnchorOnDesktop, a pure function of plain window records. #973: the
// live run counted Tao's 16x16 event-loop window at 0,0 as the anchor after the
// real anchor was parked at -32000,-32000. The records below are that run.
// Only the function is loaded, through the PowerShell parser, so the script's
// user32 calls and app launch never run.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { test } from "node:test";

import { resolvePwsh } from "./pwsh-resolve.js";

const script = join(import.meta.dirname, "..", "scripts", "verify-anchor-offscreen-win.ps1");

const resolved = resolvePwsh();
const skip = resolved.kind === "ready" ? false : resolved.reason;

const LOADER = `
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($env:ANCHOR_SCRIPT, [ref]$tokens, [ref]$errors)
if ($errors.Count -gt 0) { throw "parse errors: $($errors | Out-String)" }
foreach ($name in "Intersects", "Test-AnchorOnDesktop") {
  $fn = $ast.FindAll({ $args[0] -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $args[0].Name -eq $name }, $true)
  if ($fn.Count -ne 1) { throw "missing function $name" }
  Invoke-Expression $fn[0].Extent.Text
}
$scene = [Console]::In.ReadToEnd() | ConvertFrom-Json
foreach ($win in $scene.windows) {
  $on = Test-AnchorOnDesktop $win $scene.monitors
  Write-Output "$($win.Name)=$on"
}
`;

function decide(monitors, windows) {
  const run = spawnSync(resolved.command, [...resolved.args, "-NoProfile", "-NonInteractive", "-Command", LOADER], {
    encoding: "utf8",
    input: JSON.stringify({ monitors, windows }),
    env: { ...process.env, ANCHOR_SCRIPT: script },
  });
  assert.equal(run.status, 0, `pwsh failed:\n${run.stdout}\n${run.stderr}`);
  return Object.fromEntries(
    run.stdout
      .trim()
      .split(/\r?\n/)
      .map((line) => line.split("="))
      .map(([name, value]) => [name, value === "True"]),
  );
}

function rect(left, top, width, height) {
  return { Left: left, Top: top, Right: left + width, Bottom: top + height };
}

// DESKTOP-UQIE144 at #972: a 3440x1440 primary and a 1200x1920 portrait beside it.
const monitors = [rect(0, 0, 3440, 1440), rect(3440, 0, 1200, 1920)];

const parkedAnchor = { Name: "parkedAnchor", Class: "Tauri Window", Title: "ai-buddy", ...rect(-32000, -32000, 136, 39) };
const taoEventTarget = { Name: "taoEventTarget", Class: "Tao Thread Event Target", Title: "", ...rect(0, 0, 16, 16) };
const overlay = { Name: "overlay", Class: "Tauri Window", Title: "ai-buddy", ...rect(0, 0, 3440, 1440) };
const settings = { Name: "settings", Class: "Tauri Window", Title: "Settings", ...rect(1400, 400, 600, 520) };
const onDesktopAnchor = { Name: "onDesktopAnchor", Class: "Tauri Window", Title: "ai-buddy", ...rect(1652, 1401, 136, 39) };

test(
  "a parked anchor beside Tao's event-loop window reads as off the desktop",
  { skip },
  () => {
    assert.deepEqual(decide(monitors, [parkedAnchor, taoEventTarget, overlay, settings]), {
      parkedAnchor: false,
      taoEventTarget: false,
      overlay: false,
      settings: false,
    });
  },
);

test(
  "an anchor still on a monitor reads as on the desktop",
  { skip },
  () => {
    assert.deepEqual(decide(monitors, [onDesktopAnchor, taoEventTarget]), {
      onDesktopAnchor: true,
      taoEventTarget: false,
    });
  },
);
