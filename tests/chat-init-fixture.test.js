// Fixture test for Harness initialization: starting state visible throughout,
// then settling to ready or failed. Verifies DOM shows data-initializing attribute
// and landing copy during the initializing phase.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
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

function opening(harness) {
  return {
    name: "Atlas",
    character: "Buddy Bot",
    configured: true,
    enabled: true,
    harness: { name: "hermes", session: null, alive: false, login: null, ...harness },
    model: "",
    host: "",
    chat_ui: "minimal",
    login: null,
    harness_name: "hermes",
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
    while (!handlers.chat || document.getElementById("empty").hidden) await tick();
    const landing = document.getElementById("landing");
    const title = document.getElementById("landing-title");
    const lede = document.getElementById("landing-lede");
    const composer = document.getElementById("line");
    const report = {
      landingHidden: landing.hidden,
      dataInitializing: landing.dataset.initializing,
      title: title.textContent,
      lede: lede.textContent,
      composerPlaceholder: composer.placeholder,
      composerDisabled: composer.disabled,
    };
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify(report);
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "chat-init-fixture-"));
  const page = join(dir, "init.html");
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

test("fixture Harness with slow initialize shows starting state", { skip: !chrome }, () => {
  const result = paint(opening({ initializing: true }));

  assert.equal(result.landingHidden, false, "landing is visible");
  assert.equal(result.dataInitializing, "true", "data-initializing attribute set to true");
  assert.match(result.title, /Initializing Hermes/, "title shows initializing");
  assert.match(result.lede, /starting up/, "lede mentions starting up");
  assert.match(result.lede, /12[–-]18 seconds/, "lede mentions download time");
  assert.match(result.lede, /when the Harness answers/, "lede clarifies when ready");
  assert.equal(result.composerPlaceholder, "Starting Hermes…", "composer shows starting placeholder");
  assert.equal(result.composerDisabled, true, "composer is disabled during init");
});

test("fixture Harness transitions from initializing to ready", { skip: !chrome }, () => {
  const initializing = paint(opening({ initializing: true }));
  assert.equal(initializing.dataInitializing, "true", "starts with data-initializing true");
  assert.match(initializing.title, /Initializing/, "shows initializing title");

  const ready = paint(opening({ alive: true, initializing: false }));
  assert.equal(ready.landingHidden, true, "landing hidden when ready");
  assert.equal(ready.composerDisabled, false, "composer enabled when ready");
});

test("fixture Harness transitions from initializing to failed", { skip: !chrome }, () => {
  const initializing = paint(opening({ initializing: true }));
  assert.equal(initializing.dataInitializing, "true", "starts with data-initializing true");
  assert.match(initializing.title, /Initializing/, "shows initializing title");

  const failed = paint(opening({
    initializing: false,
    failed: "hermes exited before initialize, exit status: 1"
  }));
  assert.equal(failed.landingHidden, false, "landing still visible when failed");
  assert.equal(failed.dataInitializing, "false", "data-initializing false after failure");
  assert.match(failed.title, /failed to start/, "shows failure title");
  assert.match(failed.lede, /exited before initialize/, "lede shows failure reason");
  assert.equal(failed.composerDisabled, true, "composer stays disabled after failure");
});
