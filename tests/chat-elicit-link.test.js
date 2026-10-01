// A sign-in link a Harness hands over as a URL elicitation: Chat draws the
// link whole, and Open opens it before it tells the Harness yes. A link the
// Harness says is complete goes dead with nothing sent. Drives src/chat.html
// the way chat-landing-format.test.js does.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
const URL_ = "https://example.test/device?code=ABCD-1234";

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

const opening = {
  name: "BMO",
  character: "Buddy Bot",
  configured: true,
  enabled: true,
  harness: { name: "codex", session: null, alive: true, login: "codex login" },
  model: "",
  host: "",
  chat_ui: "minimal",
  login: "codex login",
  sign_in: [{ id: "chat-gpt-device-code", label: "ChatGPT device code" }],
  harness_name: "codex",
  instructions: "",
  personality: "",
  instance_prompt: "",
  prompt_limit: 2000,
};

const form = {
  request: "7",
  message: "Enter ABCD-1234 on the sign-in page.",
  field: "",
  options: [],
  url: URL_,
};

// Clicks Open, or with `settled` retires the row the way the Shell does on
// `elicitation/complete`: the settled event with no option.
function paint(settled = false) {
  const stub = `
<script type="module">
  const handlers = {};
  window.__invoked = [];
  window.__TAURI__ = {
    core: {
      invoke(name, args) {
        window.__invoked.push({ name, args });
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
    while (!handlers["chat-elicitation"] || document.getElementById("landing").hidden) await tick();
    handlers["chat-elicitation"]({ payload: ${JSON.stringify(form)} });
    await tick();
    const row = document.querySelector(".row.ask");
    const buttons = [...row.querySelectorAll("button")];
    const drawn = {
      code: [...row.querySelectorAll("code")].map((n) => n.textContent),
      buttons: buttons.map((b) => [b.textContent, b.dataset.option]),
      anchors: row.querySelectorAll("a").length,
    };
    const before = window.__invoked.length;
    if (${settled}) {
      handlers["chat-permission-settled"]({ payload: { request: "7", option: null } });
    } else {
      buttons.find((b) => b.textContent === "Open").click();
    }
    for (let i = 0; i < 10; i += 1) await tick();
    const calls = window.__invoked.slice(before);
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify({
      ...drawn,
      calls,
      disabled: buttons.every((b) => b.disabled),
      chosen: row.querySelectorAll(".chosen").length,
    });
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "chat-elicit-link-"));
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
      "--dump-dom",
      pathToFileURL(page).href,
    ],
    { encoding: "utf8", maxBuffer: 1 << 24, timeout: 40000, killSignal: "SIGKILL" },
  );
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the form did not report. ${run.stderr?.slice(-500) ?? ""}`);
  const json = match[1]
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
  return JSON.parse(json);
}

const skip = chrome ? false : "headless Chromium is not installed";

test("a sign-in link is drawn whole and Open opens it before answering yes", { skip, timeout: 60000 }, () => {
  const report = paint();
  assert.deepEqual(report.code, [URL_]);
  assert.deepEqual(report.buttons, [
    ["Open", "open"],
    ["Decline", "decline"],
  ]);
  assert.equal(report.anchors, 0, "an <a href> would navigate the Chat webview");
  assert.deepEqual(report.calls, [
    { name: "open_link", args: { url: URL_ } },
    { name: "elicitation_answer", args: { request: "7", value: "open" } },
  ]);
  assert.equal(report.disabled, true);
});

test("a completed link goes dead with nothing chosen and nothing sent", { skip, timeout: 60000 }, () => {
  const report = paint(true);
  assert.deepEqual(report.code, [URL_], "the row stays, so the log says what was asked");
  assert.equal(report.disabled, true);
  assert.equal(report.chosen, 0);
  assert.deepEqual(report.calls, []);
});
