// The Chat header keeps the name, the Character chip, and the mind line on one
// row at every width. As the window narrows the mind line wraps first, down to
// its min-content width; only then the Instance's name; the chip last. Drives
// src/chat.html the way chat-reply-code.test.js does and measures line boxes.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
// From wider than build_chat's inner_size, past its 320 minimum.
const WIDTHS = Array.from({ length: 29 }, (_, i) => 480 - i * 10);

function chromeBin() {
  if (process.env.AI_BUDDY_CHROME) {
    return process.env.AI_BUDDY_CHROME;
  }
  for (const name of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) {
    const found = spawnSync("bash", ["-lc", `command -v ${name}`], { encoding: "utf8" });
    const path = found.stdout.trim();
    if (found.status === 0 && path) {
      return path;
    }
  }
  return null;
}

const chrome = chromeBin();

function measure(theme, name) {
  const opening = {
    name,
    character: "Buddy Bot",
    configured: true,
    enabled: true,
    harness: {
      name: "cursor-agent",
      session: "fd4be1a2-497f-4899-a827-a4e42fbdc1f2",
      alive: true,
      missing: false,
      initializing: false,
      login: null,
    },
    model: "",
    host: "",
    chat_ui: theme,
    login: null,
    harness_name: "cursor-agent",
    instructions: "",
    personality: "",
    instance_prompt: "",
    prompt_limit: 2000,
  };
  const stub = `
<script>
  window.__TAURI__ = {
    core: {
      invoke(name) {
        if (name === "chat_opening") return Promise.resolve(${JSON.stringify(opening)});
        return Promise.resolve();
      },
    },
    event: { listen: () => Promise.resolve(() => {}) },
    webviewWindow: { getCurrentWebviewWindow: () => ({ label: "chat-test" }) },
  };
  const tick = () => new Promise((r) => setTimeout(r, 0));
  function lines(node) {
    const range = document.createRange();
    range.selectNodeContents(node);
    return new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
  }
  function minContent(node) {
    node.style.width = "min-content";
    const width = node.getBoundingClientRect().width;
    node.style.width = "";
    return width;
  }
  const side = (a, b) => a.right <= b.left + 0.5 && a.top < b.bottom && b.top < a.bottom;
  async function drive() {
    while (!document.getElementById("mind-text").textContent) await tick();
    const header = document.querySelector(".tb");
    const name = document.getElementById("name");
    const chip = document.getElementById("character");
    const mind = document.getElementById("mind");
    const text = document.getElementById("mind-text");
    const report = { mind: text.textContent, mindMin: minContent(mind), nameMin: minContent(name), widths: [] };
    for (const width of ${JSON.stringify(WIDTHS)}) {
      document.body.style.width = width + "px";
      const n = name.getBoundingClientRect();
      const c = chip.getBoundingClientRect();
      const m = mind.getBoundingClientRect();
      report.widths.push({
        width,
        oneRow: side(n, c) && side(c, m),
        clipped: text.scrollWidth > text.clientWidth + 0.5 || m.right > header.getBoundingClientRect().right + 0.5,
        name: lines(name),
        chip: lines(chip),
        mind: lines(text),
        mindWidth: m.width,
        nameWidth: n.width,
      });
    }
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify(report);
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "chat-header-wrap-"));
  const page = join(dir, "harness.html");
  const html = readFileSync(join(SRC, "chat.html"), "utf8").replace(
    "<head>",
    `<head><base href="${pathToFileURL(SRC).href}">${stub}`,
  );
  writeFileSync(page, html);

  const run = spawnSync(
    chrome,
    [
      "--headless",
      "--disable-gpu",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      `--user-data-dir=${join(dir, "profile")}`,
      "--allow-file-access-from-files",
      "--virtual-time-budget=3000",
      "--window-size=420,560",
      "--dump-dom",
      pathToFileURL(page).href,
    ],
    { encoding: "utf8", maxBuffer: 1 << 24, timeout: 40000, killSignal: "SIGKILL" },
  );
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the Chat surface did not report (${theme}). ${run.stderr?.slice(-500) ?? ""}`);
  return JSON.parse(match[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&"));
}

const THEMES = ["minimal", "terminal", "glass"];

function check(theme, report) {
  const { mindMin, nameMin } = report;
  for (const at of report.widths) {
    const where = `${theme} at ${at.width}px ${JSON.stringify({ mindMin, nameMin, ...at })}`;
    assert.equal(at.oneRow, true, `${where}: the header left one row`);
    // build_chat's minimum. Below it every part can reach its floor and the row overflows.
    if (at.width >= 320) assert.equal(at.clipped, false, `${where}: the mind line is cut off`);
    if (at.name > 1) {
      assert.ok(at.mindWidth <= mindMin + 0.5, `${where}: the name wrapped while the mind line could still give`);
    }
    if (at.chip > 1) {
      assert.ok(at.nameWidth <= nameMin + 0.5, `${where}: the chip wrapped while the name could still give`);
    }
  }
}

test(
  "the Chat header stays one row and wraps the mind line, then the name, then the chip",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 180000 },
  () => {
    for (const theme of THEMES) {
      const report = measure(theme, "Buddy Bot");
      assert.equal(report.mind, "cursor-agent · session fd4be1a2", theme);
      const widest = report.widths[0];
      const narrowest = report.widths.at(-1);
      assert.deepEqual([widest.name, widest.chip, widest.mind], [1, 1, 1], `${theme} at 480: something wrapped`);
      assert.ok(narrowest.mind > 1, `${theme} at 200: the mind line never wrapped`);
      check(theme, report);
    }
  },
);

test(
  "a name too long for the row wraps beside the chip and the mind line",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 180000 },
  () => {
    for (const theme of THEMES) {
      const report = measure(theme, "Sir Reginald Buddington the Third of Cupertino");
      const at = report.widths.find((w) => w.width === 320);
      assert.ok(at.name > 1, `${theme} at 320px ${JSON.stringify(at)}: the name did not wrap`);
      assert.equal(at.chip, 1, `${theme} at 320px ${JSON.stringify(at)}: the chip wrapped`);
      check(theme, report);
    }
  },
);
