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

// macos-latest pwsh aborts in the host (status null, not exit 134) before
// this script runs. Same family as the analyzer hook's FileLoadException
// skip: the predicate never got a chance to answer.
const PWSH_HOST_ABORT =
  /FileLoadException|Microsoft\.Management\.Infrastructure|Abort trap|not properly handled/;

function pwshHostAborted(run) {
  return PWSH_HOST_ABORT.test(`${run?.stdout ?? ""}\n${run?.stderr ?? ""}`);
}

function decide(t, monitors, windows, spawn = spawnSync) {
  const run = spawn(resolved.command, [...(resolved.args ?? []), "-NoProfile", "-NonInteractive", "-Command", LOADER], {
    encoding: "utf8",
    input: JSON.stringify({ monitors, windows }),
    env: { ...process.env, ANCHOR_SCRIPT: script },
  });
  if (run.status !== 0 && pwshHostAborted(run)) {
    t.skip("pwsh aborted before the anchor script ran");
    return null;
  }
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
  (t) => {
    const got = decide(t, monitors, [parkedAnchor, taoEventTarget, overlay, settings]);
    if (got == null) return;
    assert.deepEqual(got, {
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
  (t) => {
    const got = decide(t, monitors, [onDesktopAnchor, taoEventTarget]);
    if (got == null) return;
    assert.deepEqual(got, {
      onDesktopAnchor: true,
      taoEventTarget: false,
    });
  },
);

// macos-latest checks, job 108703393062: spawnSync status is null (not 134)
// and stderr names the truncated MMI assembly. The FileLoadException line
// alone already skipped the analyzer hook; decide() was still asserting status 0.
const MACOS_PWSH_ABORT = `An error has occurred that was not properly handled. Additional information is shown below. The PowerShell process will exit.

Unhandled exception. System.IO.FileLoadException: The given assembly name was invalid.
File name: 'Microsoft.Management.Infrastructure, Version=2.0.0.0, Culture=neutral,'
   at System.Reflection.AssemblyNameParser.Parse(ReadOnlySpan\`1 name)
   at System.Reflection.AssemblyName.ParseAsAssemblySpec(Char* pAssemblyName, Void* pAssemblySpec)`;

test("the macOS pwsh abort stderr is a host crash", () => {
  assert.equal(
    pwshHostAborted({ status: null, signal: "SIGABRT", stdout: "", stderr: MACOS_PWSH_ABORT }),
    true,
  );
  assert.equal(
    pwshHostAborted({
      status: 134,
      stdout: "",
      stderr: "Unhandled exception. System.IO.FileLoadException: The given assembly name was invalid.",
    }),
    true,
  );
  assert.equal(pwshHostAborted({ status: 1, stdout: "", stderr: "parse errors: unexpected token" }), false);
  assert.equal(pwshHostAborted({ status: null, signal: "SIGABRT", stdout: "", stderr: "" }), false);
});

test("decide skips the macOS pwsh host abort instead of failing", () => {
  let reason = "";
  const got = decide(
    {
      skip(message) {
        reason = message;
      },
    },
    monitors,
    [parkedAnchor],
    () => ({ status: null, signal: "SIGABRT", stdout: "", stderr: MACOS_PWSH_ABORT }),
  );
  assert.equal(got, null);
  assert.match(reason, /pwsh aborted before the anchor script ran/);
});

test("decide still fails a real script error", () => {
  assert.throws(
    () =>
      decide(
        {
          skip() {
            throw new Error("script errors must not skip");
          },
        },
        monitors,
        [parkedAnchor],
        () => ({ status: 1, stdout: "", stderr: "parse errors: unexpected token" }),
      ),
    /pwsh failed/,
  );
});
