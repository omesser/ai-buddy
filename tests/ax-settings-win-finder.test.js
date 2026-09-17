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
const verifySrc = readFileSync(join(root, "scripts/verify-settings-win.ps1"), "utf8");

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
        return el.controlType === "Window" && /ControlType\]::Window/.test(c.value);
      case "ProcessIdProperty":
        return el.processId === el.boundProcessId;
      default:
        throw new Error(`unknown UIA property ${c.property}`);
    }
  });
}

const finder = extractFunction(axSrc, "Find-SettingsWindow");
const nativeCut = finder.indexOf("if ($null -ne $native)");
assert.ok(nativeCut > 0, "native return must split the finder");
const nativeArm = finder.slice(0, nativeCut);
const webviewArm = finder.slice(nativeCut);

const webviewConds = parsePropertyConditions(webviewArm);
const webviewByVar = new Map(webviewConds.map((c) => [c.var, c]));
const webviewAnds = parseAndConditionArgs(webviewArm, "webviewCond");

const noPidAnd = webviewAnds.find((args) => !args.includes("pidCond"));
const withPidAnd = webviewAnds.find((args) => args.includes("pidCond"));
assert.ok(noPidAnd, "webview arm must have a no-ProcessId AndCondition");
assert.ok(withPidAnd, "webview arm must And ProcessId when the caller has one");

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
  name: "AI Buddy",
  controlType: "Window",
};

test("native AiBuddySettings class match runs before the webview fallback", () => {
  assert.match(nativeArm, /ClassNameProperty/);
  assert.match(nativeArm, /"AiBuddySettings"/);
  assert.doesNotMatch(nativeArm, /Tauri Window/);
  assert.ok(nativeArm.indexOf("FindFirst") >= 0);
  assert.ok(webviewArm.indexOf("FindFirst") >= 0);
});

test("webview AndCondition requires ClassName Tauri Window, Name Settings, and ControlType Window", () => {
  const props = webviewUnconditional.map((c) => c.property).sort();
  assert.deepEqual(props, ["ClassNameProperty", "ControlTypeProperty", "NameProperty"]);

  const byProp = Object.fromEntries(webviewUnconditional.map((c) => [c.property, c.value]));
  assert.equal(unquote(byProp.ClassNameProperty), "Tauri Window");
  assert.equal(unquote(byProp.NameProperty), "Settings");
  assert.match(byProp.ControlTypeProperty, /ControlType\]::Window/);
});

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

test("optional ProcessId is AND-ed onto both arms and is not named Pid", () => {
  assert.match(axSrc, /\[Parameter\(Position=1\)\]\s*\[int\]\$ProcessId = 0/);
  assert.doesNotMatch(axSrc, /param\([^)]*\$Pid\b/s);

  const nativeAnds = parseAndConditionArgs(nativeArm, "nativeCond");
  assert.ok(
    nativeAnds.some((args) => args.includes("pidCond")),
    "native arm must And ProcessId when known",
  );
  assert.deepEqual(withPidAnd.slice(0, 3), noPidAnd);
  assert.equal(withPidAnd.at(-1), "pidCond");

  const pidCond = webviewByVar.get("pidCond");
  assert.equal(pidCond.property, "ProcessIdProperty");
  assert.equal(pidCond.value, "$ProcessId");
});

test("verify-settings-win.ps1 webview path also requires Tauri Window class and Settings title", () => {
  const nativeIdx = verifySrc.indexOf('$className -eq "AiBuddySettings"');
  const tauriIdx = verifySrc.indexOf('$className -eq "Tauri Window"');
  assert.ok(nativeIdx >= 0, "verify script still prefers AiBuddySettings");
  assert.ok(tauriIdx > nativeIdx, "webview fallback comes after native class match");
  const tauriBlock = verifySrc.slice(tauriIdx, tauriIdx + 800);
  assert.match(tauriBlock, /\$txt\.ToString\(\) -eq "Settings"/);
});
