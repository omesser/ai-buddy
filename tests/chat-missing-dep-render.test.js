// A missing launcher, driven through the Chat surface headless the way
// chat-thinking-render.test.js drives a turn. ADR-0035: the error stays up,
// selectable, until an attach answers or the user picks another Harness.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));

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

const opening = (name, harness) => ({
  name: "Buddy Bot",
  character: "Buddy Bot",
  configured: true,
  enabled: true,
  harness: { name, session: null, alive: false, missing: null, initializing: false, failed: null, login: null, ...harness },
  model: "",
  host: "",
  chat_ui: "minimal",
  login: null,
  harness_name: name,
  instructions: "",
  personality: "",
  instance_prompt: "",
  prompt_limit: 2000,
});

const MISSING = opening("claude", { missing: "npx", install: "https://nodejs.org/" });
const RETRYING = opening("claude", { missing: "npx", install: "https://nodejs.org/", initializing: true });
const ANSWERING = opening("claude", { alive: true, session: "fd4be1a2" });
const SWITCHED = opening("hermes", { initializing: true });

function drive() {
  const stub = `
<script>
  const heard = {};
  window.__TAURI__ = {
    core: {
      invoke(name) {
        if (name === "chat_opening") return Promise.resolve(${JSON.stringify(MISSING)});
        return Promise.resolve();
      },
    },
    event: {
      listen(name, handler) {
        heard[name] = handler;
        return Promise.resolve(() => {});
      },
    },
    webviewWindow: { getCurrentWebviewWindow: () => ({ label: "chat-test" }) },
  };
  const tick = () => new Promise((r) => setTimeout(r, 0));
  const open = (payload) => heard["chat-opening"]({ payload });
  function selectable(node) {
    for (let at = node; at; at = at.parentElement) {
      const style = getComputedStyle(at);
      if ((style.userSelect || style.webkitUserSelect) === "none") return false;
    }
    return true;
  }
  function seen() {
    const lede = document.getElementById("landing-lede");
    return {
      landing: !document.getElementById("empty").hidden && !document.getElementById("landing").hidden,
      title: document.getElementById("landing-title").textContent,
      lede: lede.textContent,
      links: [...lede.querySelectorAll(".md-link")].map((link) => link.dataset.href),
      select: selectable(lede),
      mind: document.getElementById("mind-text").textContent,
      composer: !document.getElementById("line").disabled,
    };
  }
  async function run() {
    while (!heard["chat-opening"]) await tick();
    while (document.getElementById("landing").hidden) await tick();
    const report = { missing: seen() };
    await new Promise((r) => setTimeout(r, 20000));
    report.later = seen();
    open(${JSON.stringify(RETRYING)});
    report.retrying = seen();
    open(${JSON.stringify(MISSING)});
    open(${JSON.stringify(ANSWERING)});
    report.answering = seen();
    open(${JSON.stringify(MISSING)});
    open(${JSON.stringify(SWITCHED)});
    report.switched = seen();
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify(report);
    document.body.append(out);
  }
  window.addEventListener("load", () => run().catch((why) => {
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify({ error: String(why) });
    document.body.append(out);
  }));
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "chat-missing-dep-render-"));
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
      "--virtual-time-budget=30000",
      "--window-size=420,560",
      "--dump-dom",
      pathToFileURL(page).href,
    ],
    { encoding: "utf8", maxBuffer: 1 << 24, timeout: 40000, killSignal: "SIGKILL" },
  );
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the Chat surface did not report. ${run.stderr?.slice(-500) ?? ""}`);
  return JSON.parse(match[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">"));
}

test(
  "a missing launcher stays on the landing until an attach answers or the Harness changes",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 60000 },
  () => {
    const report = drive();
    assert.equal(report.error, undefined, report.error);

    const missing = {
      landing: true,
      title: "Claude Code needs npx",
      lede: "npx is not installed. Fidget does not bundle npx. Install from https://nodejs.org/. Then press Claude Code again, or pick a different Harness below.",
      links: ["https://nodejs.org/"],
      select: true,
      mind: "claude · npx is not installed",
      composer: false,
    };
    assert.deepEqual(report.missing, missing);
    assert.deepEqual(report.later, missing, "twenty seconds later nothing has cleared it");
    assert.deepEqual(report.retrying, missing, "a retry in flight keeps the error up");

    assert.equal(report.answering.landing, false, "an attach that answers clears it");
    assert.equal(report.answering.composer, true);

    assert.equal(report.switched.landing, true);
    assert.equal(report.switched.title, "Initializing Hermes…", "a different Harness replaces it");
    assert.deepEqual(report.switched.links, []);
  },
);
