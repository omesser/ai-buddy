// The landing's commands read as code, its URLs open in the browser, and all
// of it copies as plain text. Drives src/chat.html the way
// chat-reply-code.test.js does; a real drag and paste still need a person.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
const FAILED =
  "`npx` exited before initialize, exit status: 1. `npx` runs on Node.js: run `node --version` in a terminal to check that it starts.";

function chromeBin() {
  if (process.env.FIDGET_CHROME) {
    return process.env.FIDGET_CHROME;
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

function opening(harness) {
  return {
    name: "BMO",
    character: "Buddy Bot",
    configured: true,
    enabled: true,
    harness: { name: "codex", session: null, alive: false, login: null, ...harness },
    model: "",
    host: "",
    chat_ui: "minimal",
    login: null,
    harness_name: "codex",
    instructions: "",
    personality: "",
    instance_prompt: "",
    prompt_limit: 2000,
  };
}

function paint(open) {
  const stub = `
<script type="module">
  const handlers = {};
  window.__invoked = [];
  window.__TAURI__ = {
    core: {
      invoke(name, args) {
        window.__invoked.push({ name, args });
        if (name === "chat_opening") return Promise.resolve(${JSON.stringify(open)});
        return Promise.resolve();
      },
    },
    event: {
      listen(name, handler) {
        handlers[name] = handler;
        return Promise.resolve(() => {});
      },
    },
    webviewWindow: { getCurrentWebviewWindow: () => ({ label: "chat-test" }) },
  };
  const tick = () => new Promise((r) => setTimeout(r, 0));
  async function drive() {
    const title = document.getElementById("landing-title");
    const lede = document.getElementById("landing-lede");
    while (!handlers.chat || document.getElementById("empty").hidden) await tick();
    const range = document.createRange();
    range.setStartBefore(title);
    range.setEndAfter(lede);
    getSelection().removeAllRanges();
    getSelection().addRange(range);
    const selected = getSelection().toString();
    const link = lede.querySelector(".md-link");
    const code = lede.querySelector("code");
    let clicked = null;
    if (link) {
      const before = window.__invoked.length;
      link.click();
      clicked = window.__invoked.slice(before).find((call) => call.name === "open_link") ?? null;
    }
    const style = (node) => {
      if (!node) return null;
      const s = getComputedStyle(node);
      return { font: s.fontFamily, color: s.color, line: s.textDecorationLine, select: s.userSelect };
    };
    const mind = document.getElementById("mind-text");
    const report = {
      title: title.textContent,
      titleCode: [...title.querySelectorAll("code")].map((n) => n.textContent),
      ledeCode: [...lede.querySelectorAll("code")].map((n) => n.textContent),
      mindCode: [...mind.querySelectorAll("code")].map((n) => n.textContent),
      backticks: (title.textContent + lede.textContent + mind.textContent).includes("\`"),
      links: [...lede.querySelectorAll(".md-link")].map((n) => n.dataset.href),
      anchors: document.querySelectorAll("#landing a").length,
      selected,
      clicked,
      lede: style(lede),
      code: style(code),
      link: style(link),
      accent: getComputedStyle(document.documentElement).getPropertyValue("--chat-accent").trim(),
    };
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify(report);
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "chat-landing-format-"));
  const page = join(dir, "harness.html");
  const html = spawnSync("cat", [join(SRC, "chat.html")], { encoding: "utf8" }).stdout.replace(
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
      "--dump-dom",
      pathToFileURL(page).href,
    ],
    { encoding: "utf8", maxBuffer: 1 << 24, timeout: 40000, killSignal: "SIGKILL" },
  );
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the landing did not report. ${run.stderr?.slice(-500) ?? ""}`);
  const json = match[1]
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
  return JSON.parse(json);
}

const skip = chrome ? false : "headless Chromium is not installed";

test("a launcher that failed to start shows its commands as code", { skip, timeout: 60000 }, () => {
  const report = paint(opening({ failed: FAILED }));
  assert.equal(report.backticks, false);
  assert.deepEqual(report.ledeCode, ["npx", "npx", "node --version"]);
  assert.match(report.code.font, /mono/i);
  assert.notEqual(report.lede.select, "none");
  assert.match(report.selected, /npx exited before initialize/);
  assert.match(report.selected, /run node --version in a terminal/);
  assert.doesNotMatch(report.selected, /`/);
});

test("a missing launcher links its install page to the browser", { skip, timeout: 60000 }, () => {
  const report = paint(opening({ missing: "npx", install: "https://nodejs.org/" }));
  assert.equal(report.title, "Codex needs npx");
  assert.deepEqual(report.titleCode, ["npx"]);
  assert.deepEqual(report.mindCode, ["npx"]);
  assert.equal(report.backticks, false);
  assert.deepEqual(report.links, ["https://nodejs.org/"]);
  assert.equal(report.anchors, 0, "an <a href> would navigate the Chat webview");
  assert.equal(report.link.line, "underline");
  assert.notEqual(report.link.color, report.lede.color, "a link is not coloured apart from its sentence");
  assert.deepEqual(report.clicked, { name: "open_link", args: { url: "https://nodejs.org/" } });
  assert.match(report.selected, /Install from https:\/\/nodejs\.org\/\./);
  assert.match(report.selected, /npx is not installed/);
});
