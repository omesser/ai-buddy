// The unavailable pill's Open chat link opens Chat and is not a Poke. Drives the
// real src/index.html and main.js the way chat-elicit-link.test.js drives Chat.

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

// Nothing configured, so nothing can answer and nothing is starting.
const opening = { configured: false, enabled: true, harness: null, login: "" };

const frame = {
  sprites: [
    {
      id: "bmo",
      character: "Buddy Bot",
      animation: "idle",
      frame_index: 0,
      x: 200,
      y: 200,
      width: 64,
      height: 64,
      mirror: 1,
      bubble: true,
    },
  ],
  visible: true,
  fade_ms: 0,
  sound: false,
};

function press() {
  const stub = `
<script type="module">
  const handlers = {};
  window.__invoked = [];
  window.__TAURI__ = {
    core: {
      invoke(name, args) {
        window.__invoked.push({ name, args });
        if (name === "character") return Promise.resolve({ characters: {} });
        if (name === "overlay_hit_tests_hotspots") return Promise.resolve(true);
        if (name === "chat_opening") return Promise.resolve(${JSON.stringify(opening)});
        return Promise.resolve();
      },
    },
    event: {
      listen(name, handler) {
        handlers[name] = handler;
        return Promise.resolve(() => {});
      },
    },
    webviewWindow: { getCurrentWebviewWindow: () => ({ label: "overlay-test" }) },
  };
  const tick = () => new Promise((r) => setTimeout(r, 50));
  async function drive() {
    while (!handlers.frame) await tick();
    handlers.frame({ payload: ${JSON.stringify(frame)} });
    await tick();
    const quick = document.querySelector('.quick-message[data-instance="bmo"]');
    document.querySelector('.sprite[data-instance="bmo"]').dispatchEvent(new PointerEvent("pointerenter"));
    while (!quick.classList.contains("visible")) await tick();
    const link = quick.querySelector(".quick-message-connect");
    const openChat = link.querySelector(".quick-message-open");
    const send = quick.querySelector(".quick-message-send");
    const shown = {
      hint: link.firstChild.textContent.trim(),
      control: openChat.textContent,
      linkShown: !link.hidden,
      sendShown: !send.hidden && getComputedStyle(send).display !== "none",
    };
    const before = window.__invoked.length;
    const at = { bubbles: true, button: 0, pointerId: 1, isPrimary: true };
    openChat.dispatchEvent(new PointerEvent("pointerdown", at));
    openChat.dispatchEvent(new PointerEvent("pointerup", at));
    openChat.click();
    for (let i = 0; i < 5; i += 1) await tick();
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify({
      ...shown,
      clicks: window.__invoked
        .slice(before)
        .filter((c) => ["overlay_primary", "overlay_secondary", "overlay_open_chat"].includes(c.name)),
      open: quick.classList.contains("visible"),
    });
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "quick-message-connect-"));
  const page = join(dir, "harness.html");
  const html = readFileSync(join(SRC, "index.html"), "utf8").replace(
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
      "--virtual-time-budget=8000",
      "--dump-dom",
      pathToFileURL(page).href,
    ],
    { encoding: "utf8", maxBuffer: 1 << 24, timeout: 40000, killSignal: "SIGKILL" },
  );
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the overlay did not report. ${run.stderr?.slice(-500) ?? ""}`);
  const json = match[1]
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
  return JSON.parse(json);
}

const skip = chrome ? false : "headless Chromium is not installed";

test("a click on Open chat opens Chat, closes the pill, and is no Poke", { skip, timeout: 60000 }, () => {
  const report = press();
  assert.equal(report.hint, "Connect an AI to talk to me");
  assert.equal(report.control, "Open chat");
  assert.equal(report.linkShown, true);
  assert.equal(report.sendShown, false, "Send is hidden while the link is up");
  assert.deepEqual(report.clicks, [{ name: "overlay_open_chat", args: { id: "bmo" } }]);
  assert.equal(report.open, false);
});
