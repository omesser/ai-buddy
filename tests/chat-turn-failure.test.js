// A turn the Harness failed reads as a Harness error, with the Harness's own
// words boxed, and not as a bare note. Drives src/chat.html the way
// chat-reply-code.test.js does.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
// What grok answered every `session/prompt` with once its balance ran out,
// as `acp_wire::error_text` renders it.
const BALANCE_EXHAUSTED =
  "Internal error: API error (status 402 Payment Required): Grok Build usage balance exhausted";
const TIMEOUT = "harness turn exceeded 60s; cancelled";

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

function turnFailurePage(payloads) {
  const opening = {
    name: "BMO",
    character: "Buddy Bot",
    configured: true,
    enabled: true,
    harness: { name: "grok", alive: true, missing: null, initializing: false, login: null },
    model: "",
    host: "",
    chat_ui: "minimal",
    login: null,
    harness_name: "grok",
    instructions: "",
    personality: "",
    instance_prompt: "",
    prompt_limit: 2000,
  };
  const stub = `
<script type="module">
  const handlers = {};
  window.__TAURI__ = {
    core: {
      invoke(name) {
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
    webviewWindow: { getCurrentWebviewWindow: () => ({ label: "chat-test" }) },
  };
  const tick = () => new Promise((r) => setTimeout(r, 0));
  async function drive() {
    for (const payload of ${JSON.stringify(payloads)}) {
      while (!handlers.chat || document.getElementById("line").disabled) await tick();
      const asked = document.querySelectorAll("#log .row.you").length;
      document.getElementById("line").value = "are you there?";
      document.getElementById("composer").requestSubmit();
      while (document.querySelectorAll("#log .row.you").length === asked) await tick();
      handlers.chat({
        payload: { said: null, busy: false, reacting_to: null, you: false, at: null, ...payload },
      });
      await tick();
    }
    const log = document.getElementById("log");
    const failed = log.querySelector(".harness-error");
    const report = {
      notes: [...log.querySelectorAll(".note:not(.harness-error)")].map((n) => n.lastChild.textContent),
      kicker: failed?.querySelector(".kicker")?.textContent ?? null,
      title: failed?.querySelector("h2")?.textContent ?? null,
      lede: failed?.querySelector("p:not(.kicker)")?.textContent ?? null,
      label: failed?.querySelector(".failure-part h3")?.textContent ?? null,
      box: failed?.querySelector("pre.failure-box.error")?.textContent ?? null,
      boxFont: failed ? getComputedStyle(failed.querySelector("pre")).fontFamily : null,
      sideways: Math.max(
        document.documentElement.scrollWidth - document.documentElement.clientWidth,
        log.scrollWidth - log.clientWidth,
      ),
    };
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify(report);
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;
  const dir = mkdtempSync(join(tmpdir(), "chat-turn-failure-"));
  const page = join(dir, "harness.html");
  const html = spawnSync("cat", [join(SRC, "chat.html")], { encoding: "utf8" }).stdout.replace(
    "<head>",
    `<head><base href="${pathToFileURL(SRC).href}">${stub}`,
  );
  writeFileSync(page, html);
  return { dir, page };
}

function paint(payloads, width) {
  const { dir, page } = turnFailurePage(payloads);
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
      `--window-size=${width},560`,
      "--dump-dom",
      pathToFileURL(page).href,
    ],
    { encoding: "utf8", maxBuffer: 1 << 24, timeout: 40000, killSignal: "SIGKILL" },
  );
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the Chat surface did not report. ${run.stderr?.slice(-500) ?? ""}`);
  const json = match[1]
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
  return JSON.parse(json);
}

const skip = chrome ? false : "headless Chromium is not installed";

test("a turn the Harness failed shows its words in a Harness error box", { skip, timeout: 90000 }, () => {
  // The narrowest Chat, where a long unbroken error would push it sideways.
  const report = paint([{ error: BALANCE_EXHAUSTED, failure: BALANCE_EXHAUSTED }], 320);
  assert.equal(report.kicker, "Harness error");
  assert.equal(report.title, "Grok couldn't answer");
  assert.equal(report.lede, "It sent back an error instead of a reply:");
  assert.equal(report.label, "Error output");
  assert.equal(report.box, BALANCE_EXHAUSTED);
  assert.match(report.boxFont, /mono/i);
  assert.deepEqual(report.notes, [], "the error is not also a note");
  assert.equal(report.sideways, 0, "the box pushed Chat sideways");
});

test("an error the Shell names stays a note", { skip, timeout: 90000 }, () => {
  const report = paint([{ error: TIMEOUT }], 420);
  assert.deepEqual(report.notes, [TIMEOUT]);
  assert.equal(report.kicker, null);
});
