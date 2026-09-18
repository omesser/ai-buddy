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
const phase2Src = readFileSync(
  join(root, "scripts/verify-settings-webview-phase2-win.ps1"),
  "utf8",
);

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

test("FromHandle helper is the multi-monitor UIA entry, not RootElement", () => {
  const helper = extractFunction(axSrc, "Get-AutomationElementFromHandle");
  assert.match(helper, /AutomationElement\]::FromHandle/);
  assert.doesNotMatch(helper, /AutomationElement\]::RootElement/);
  assert.match(helper, /multi-monitor/);
  assert.match(finder, /WindowHandle/);
  assert.match(finder, /Get-AutomationElementFromHandle/);
  assert.match(finder, /multi-monitor/);
});

test("phase2 webview smoke does not replace native verify-settings-win.ps1", () => {
  assert.match(phase2Src, /Does not replace scripts\/verify-settings-win\.ps1/);
  assert.match(verifySrc, /AiBuddySettings/);
});

test("phase2 smoke never assigns $PID and uses ProcessId names", () => {
  assert.doesNotMatch(phase2Src, /\$pid\s*=/i);
  assert.match(phase2Src, /\$targetProcessId/);
  assert.match(phase2Src, /Constant\+AllScope/);
});

test("phase2 UIA is FromHandle-only and parks via work area", () => {
  assert.match(phase2Src, /AutomationElement\]::FromHandle/);
  assert.doesNotMatch(phase2Src, /AutomationElement\]::RootElement/);
  assert.match(phase2Src, /EnumDisplayMonitors/);
  assert.match(phase2Src, /No secondary monitor/);
  assert.match(phase2Src, /GetWindowRect/);
  assert.doesNotMatch(phase2Src, /VirtualScreen/);
  assert.doesNotMatch(phase2Src, /::SetCursorPos/);
});

test("phase2 CHECK2 tries LegacyIAccessible before PostMessage", () => {
  const activate = extractFunction(phase2Src, "Invoke-UiaActivate");
  const invokeAt = activate.indexOf("InvokePattern");
  const selectAt = activate.indexOf("SelectionItemPattern");
  const legacyAt = activate.indexOf("LegacyIAccessiblePattern");
  const postAt = activate.indexOf("[Phase2Win]::PostMessage");
  assert.ok(invokeAt >= 0 && selectAt > invokeAt, "Invoke then SelectionItem");
  assert.ok(legacyAt > selectAt, "LegacyIAccessible after the two missing WebView2 patterns");
  assert.match(activate, /DoDefaultAction/);
  assert.ok(postAt > legacyAt, "PostMessage last, still no SetCursorPos");
});

test("phase2 CHECK2 waits for TabItem names then finds TabItem AND Name", () => {
  const waiter = extractFunction(phase2Src, "Wait-SettingsTabs");
  assert.match(waiter, /TimeoutMs = 8000/);
  assert.match(waiter, /PollMs = 350/);
  assert.match(waiter, /Find-SettingsTab/);
  assert.doesNotMatch(waiter, /AutomationElement\]::RootElement/);

  const tabFinder = extractFunction(phase2Src, "Find-SettingsTab");
  assert.match(tabFinder, /New-TabItemAndNameCondition/);
  assert.doesNotMatch(tabFinder, /AutomationElement\]::RootElement/);

  const condFn = extractFunction(phase2Src, "New-TabItemAndNameCondition");
  const conds = parsePropertyConditions(condFn);
  const byVar = new Map(conds.map((c) => [c.var, c]));
  const ands = parseAndConditionArgs(condFn, "andCond");
  assert.equal(ands.length, 1);
  assert.deepEqual(ands[0].slice().sort(), ["nameCond", "typeCond"]);
  assert.equal(byVar.get("nameCond").property, "NameProperty");
  assert.equal(byVar.get("nameCond").value, "$Name");
  assert.equal(byVar.get("typeCond").property, "ControlTypeProperty");
  assert.match(byVar.get("typeCond").value, /ControlType\]::TabItem/);

  const tabItemAndName = [
    { property: "NameProperty", value: '"Presence"' },
    byVar.get("typeCond"),
  ];
  const presencePane = { name: "Presence", controlType: "Pane" };
  const presenceTab = { name: "Presence", controlType: "TabItem" };
  assert.equal(matches(presencePane, tabItemAndName), false, "Pane named Presence is not a tab");
  assert.equal(matches(presenceTab, tabItemAndName), true);
  assert.match(phase2Src, /already selected \(activate no-op/);
});

test("phase2 reports the four #715 checks and crops evidence to HWND", () => {
  assert.match(phase2Src, /1_window_opens/);
  assert.match(phase2Src, /2_five_tabs/);
  assert.match(phase2Src, /3_roundtrip/);
  assert.match(phase2Src, /4_zorder/);
  assert.match(phase2Src, /ai-buddy\\settings\.json/);
  const capture = extractFunction(phase2Src, "Capture");
  assert.match(capture, /GetWindowRect/);
  assert.match(capture, /CopyFromScreen/);
  assert.match(capture, /\/ 3\.0/);
});

function hwndAbove(front, back, stack) {
  const fi = stack.indexOf(front);
  const bi = stack.indexOf(back);
  return fi >= 0 && bi >= 0 && fi < bi;
}

function settingsAboveOverlays(settings, overlays, stack) {
  if (!overlays.length) return false;
  return overlays.every((ov) => hwndAbove(settings, ov, stack));
}

function isOverlay(el, settingsHwnd, minW, minH) {
  if (el.hwnd === settingsHwnd) return false;
  if (el.title === "Settings") return false;
  if (el.title !== "ai-buddy") return false;
  if (el.processId !== el.boundProcessId) return false;
  return el.width >= minW && el.height >= minH;
}

test("phase2 check 4 is PASS/FAIL from GetWindow stacking, not REVIEW", () => {
  const walk = extractFunction(phase2Src, "Get-ZOrderHwnds");
  assert.match(walk, /GetTopWindow/);
  assert.match(walk, /GW_HWNDNEXT/);
  assert.match(phase2Src, /public const uint GW_HWNDNEXT = 2/);

  const above = extractFunction(phase2Src, "Test-HwndAbove");
  assert.match(above, /Get-HwndStackIndex/);
  assert.match(above, /-lt/);

  const find = extractFunction(phase2Src, "Find-OverlayHwnds");
  assert.match(find, /ai-buddy/);
  assert.match(find, /GetWindowRect/);
  assert.match(find, /\$t -eq 'Settings'/);

  assert.match(phase2Src, /Capture '04-zorder\.png'/);
  assert.match(
    phase2Src,
    /\$Report\.checks\['4_zorder'\] = \$\(if \(\$zOk\) \{ 'PASS' \} else \{ 'FAIL' \}\)/,
  );
  assert.doesNotMatch(phase2Src, /4_zorder'\] = 'REVIEW'/);
  assert.doesNotMatch(phase2Src, /Judge Settings above/);
});

test("stacking fixtures: Settings before overlay is above; missing HWND is not", () => {
  assert.equal(hwndAbove("settings", "overlay", ["settings", "overlay", "other"]), true);
  assert.equal(hwndAbove("settings", "overlay", ["overlay", "settings"]), false);
  assert.equal(hwndAbove("settings", "overlay", ["settings"]), false);
  assert.equal(hwndAbove("settings", "overlay", ["overlay"]), false);
  assert.equal(
    settingsAboveOverlays("settings", ["ov1", "ov2"], ["settings", "ov1", "ov2"]),
    true,
  );
  assert.equal(
    settingsAboveOverlays("settings", ["ov1", "ov2"], ["ov1", "settings", "ov2"]),
    false,
  );
  assert.equal(settingsAboveOverlays("settings", [], ["settings"]), false);

  const all = extractFunction(phase2Src, "Test-SettingsAboveOverlays");
  assert.match(all, /Test-HwndAbove/);
  assert.match(all, /Count -eq 0/);
});

test("overlay finder fixtures: large ai-buddy matches; Settings, 1x1 anchor, and Chat do not", () => {
  const minW = 400;
  const minH = 400;
  const settingsHwnd = 1;
  const tauriSettings = {
    hwnd: 1,
    title: "Settings",
    width: 600,
    height: 520,
    processId: 4242,
    boundProcessId: 4242,
  };
  const overlay = {
    hwnd: 2,
    title: "ai-buddy",
    width: 1920,
    height: 1080,
    processId: 4242,
    boundProcessId: 4242,
  };
  const anchor = {
    hwnd: 3,
    title: "ai-buddy",
    width: 1,
    height: 1,
    processId: 4242,
    boundProcessId: 4242,
  };
  const chat = {
    hwnd: 4,
    title: "Timber Wolf",
    width: 420,
    height: 560,
    processId: 4242,
    boundProcessId: 4242,
  };
  const otherPid = { ...overlay, hwnd: 5, processId: 99, boundProcessId: 4242 };
  assert.equal(isOverlay(overlay, settingsHwnd, minW, minH), true);
  assert.equal(isOverlay(tauriSettings, settingsHwnd, minW, minH), false);
  assert.equal(isOverlay(anchor, settingsHwnd, minW, minH), false);
  assert.equal(isOverlay(chat, settingsHwnd, minW, minH), false);
  assert.equal(isOverlay(otherPid, settingsHwnd, minW, minH), false);

  const find = extractFunction(phase2Src, "Find-OverlayHwnds");
  assert.match(find, /\$t -ne 'ai-buddy'/);
  assert.match(find, /\$w -ge \$script:overlayMinW/);
  assert.match(find, /\$hgt -ge \$script:overlayMinH/);
  assert.match(find, /\$processId -ne \$script:overlayPid/);
});
