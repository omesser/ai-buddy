// Architect #805: the webview arm matched any Window named Settings, which is
// also the Windows Settings app. Live UIA is not on this VM or in CI, so this
// parses Find-SettingsWindow's conditions and applies them to fixtures.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const axSrc = readFileSync(join(root, "scripts/ax-settings-win.ps1"), "utf8");

function extractFunction(src, name) {
  const marker = `function ${name}`;
  const start = src.indexOf(marker);
  assert.ok(start >= 0, `missing function ${name}`);
  const brace = src.indexOf("{", start);
  let depth = 0;
  for (let i = brace; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  throw new Error(`unclosed function ${name}`);
}

function parsePropertyConditions(body) {
  const re =
    /\$(\w+)\s*=\s*New-Object System\.Windows\.Automation\.PropertyCondition\(\s*\[System\.Windows\.Automation\.AutomationElement\]::(\w+)\s*,\s*([^)]+?)\s*\)/g;
  return [...body.matchAll(re)].map((m) => ({
    var: m[1],
    property: m[2],
    value: m[3].trim(),
  }));
}

function parseAndConditionArgs(body, targetVar) {
  const re = new RegExp(
    `\\$${targetVar}\\s*=\\s*New-Object System\\.Windows\\.Automation\\.AndCondition\\(([^)]+)\\)`,
    "g",
  );
  return [...body.matchAll(re)].map((m) =>
    m[1]
      .split(",")
      .map((part) => part.trim().replace(/^\$/, ""))
      .filter(Boolean),
  );
}

function unquote(value) {
  const m = value.match(/^"(.*)"$/);
  return m ? m[1] : value;
}

function matches(el, conds) {
  return conds.every((c) => {
    switch (c.property) {
      case "ClassNameProperty":
        return el.className === unquote(c.value);
      case "NameProperty":
        return el.name === unquote(c.value);
      case "ControlTypeProperty":
        if (/ControlType\]::TabItem/.test(c.value)) return el.controlType === "TabItem";
        return el.controlType === "Window" && /ControlType\]::Window/.test(c.value);
      case "ProcessIdProperty":
        return el.processId === el.boundProcessId;
      default:
        throw new Error(`unknown UIA property ${c.property}`);
    }
  });
}

const finder = extractFunction(axSrc, "Find-SettingsWindow");

const webviewConds = parsePropertyConditions(finder);
const webviewByVar = new Map(webviewConds.map((c) => [c.var, c]));
const webviewAnds = parseAndConditionArgs(finder, "webviewCond");

const noPidAnd = webviewAnds.find((args) => !args.includes("pidCond"));
assert.ok(noPidAnd, "webview arm must have a no-ProcessId AndCondition");

const webviewUnconditional = noPidAnd.map((name) => {
  const cond = webviewByVar.get(name);
  assert.ok(cond, `AndCondition names unknown $${name}`);
  return cond;
});

const windowsSettingsApp = {
  className: "ApplicationFrameWindow",
  name: "Settings",
  controlType: "Window",
};
const tauriSettings = {
  className: "Tauri Window",
  name: "Settings",
  controlType: "Window",
  processId: 4242,
  boundProcessId: 4242,
};
const otherTauri = {
  className: "Tauri Window",
  name: "Fidget",
  controlType: "Window",
};

test("Name-only is insufficient: the Windows Settings app is not a match", () => {
  const nameOnly = webviewUnconditional.filter(
    (c) => c.property === "NameProperty" || c.property === "ControlTypeProperty",
  );
  assert.equal(nameOnly.length, 2, "the old two-property matcher is the contrast fixture");
  assert.equal(
    matches(windowsSettingsApp, nameOnly),
    true,
    "Name+Window alone still hits the Windows Settings app",
  );
  assert.equal(matches(windowsSettingsApp, webviewUnconditional), false);
  assert.equal(matches(tauriSettings, webviewUnconditional), true);
  assert.equal(matches(otherTauri, webviewUnconditional), false);
});

// DROPPED (#1268): "webview AndCondition requires ClassName..." compares parsed property names. The Name-only match stays.
// DROPPED (#1268): "optional ProcessId is AND-ed..." greps for $ProcessId and pidCond. It does not run the finder.
// DROPPED (#1268): "FromHandle helper is the multi-monitor UIA entry" greps for FromHandle and the absence of RootElement.
// DROPPED (#1268): "phase2 relies on the webview default" asserts the script does not mention the settings env flags.
// DROPPED (#1268): "phase2 smoke never assigns $PID" greps for $targetProcessId and forbids $pid =.
// DROPPED (#1268): "phase2 UIA is FromHandle-only" greps for EnumDisplayMonitors and the absence of SetCursorPos.
// DROPPED (#1268): "phase2 CHECK2 tries LegacyIAccessible before PostMessage" asserts that source order.
// DROPPED (#1268): "phase2 CHECK2 waits for TabItem names" greps the waiter for TimeoutMs 8000 and condition names.
// DROPPED (#1268): "phase2 reports the four #715 checks" greps for the check id strings and CopyFromScreen.
// DROPPED (#1268): "phase2 check 4 is PASS/FAIL from GetWindow stacking" greps GetTopWindow. The walk is not executed.
// DROPPED (#1268): "stacking fixtures" asserts a JS copy of the walk, then greps for Test-HwndAbove.
// DROPPED (#1268): "overlay finder fixtures" asserts a JS copy of isOverlay, then greps width constants.
