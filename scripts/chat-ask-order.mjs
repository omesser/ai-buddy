#!/usr/bin/env node
// Drives the real Chat surface (src/chat.html + chat.js) in headless Chromium
// with `window.__TAURI__` stubbed, replays one typed turn that raises a
// permission ask and then answers, and prints the log rows in DOM order.
// Exit 1 when the answer row sits above the ask row it answered after.
//   node scripts/chat-ask-order.mjs
// Env:
//   AI_BUDDY_CHROME  the headless Chromium binary (default: Playwright's shell)

import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
const CHROME =
  process.env.AI_BUDDY_CHROME ??
  join(
    homedir(),
    "Library/Caches/ms-playwright/chromium_headless_shell-1228/chrome-headless-shell-mac-arm64/chrome-headless-shell",
  );

const OPENING = {
  name: "BMO",
  character: "Buddy Bot",
  configured: true,
  enabled: true,
  harness: { alive: true, missing: false, initializing: false },
  model: "",
  host: "",
  chat_ui: "minimal",
  login: null,
  harness_name: "Claude Code",
  instructions: "",
  personality: "",
  instance_prompt: "",
  prompt_limit: 2000,
};

const ASK = {
  request: "5",
  title: "mcp__ai-buddy__list_windows",
  kind: "other",
  content: [],
  input: null,
  locations: [],
  options: [
    { id: "allow-once", name: "Yes", kind: "allow_once" },
    { id: "allow-with-updates", name: "Yes, and don't ask again", kind: "allow_always" },
    { id: "reject-once", name: "No", kind: "reject_once" },
  ],
};

const ANSWER = "Hmm. BMO still sees only shapes, no names.";

const stub = `
<script>
  const handlers = {};
  const calls = [];
  window.__TAURI__ = {
    core: {
      invoke(name, args) {
        calls.push(name);
        if (name === "chat_opening") return Promise.resolve(${JSON.stringify(OPENING)});
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
    while (!handlers["chat-permission-settled"] || document.getElementById("line").disabled) {
      await tick();
    }
    document.getElementById("line").value = "ok, tell me what you see now";
    document.getElementById("composer").requestSubmit();
    while (!calls.includes("chat_send")) await tick();
    handlers["chat-permission"]({ payload: ${JSON.stringify(ASK)} });
    await tick();
    document.querySelector('[data-option="allow-with-updates"]').click();
    while (!calls.includes("permission_answer")) await tick();
    handlers["chat-permission-settled"]({ payload: { request: "5", option: "allow-with-updates" } });
    handlers["chat"]({ payload: { said: ${JSON.stringify(ANSWER)}, busy: false, reacting_to: null, you: false, at: null, error: null, superseded_by: null } });
    await tick();
    const rows = [...document.querySelectorAll("#log .row")].map((row) => ({
      who: row.querySelector(".who-label")?.textContent ?? "",
      said: row.querySelector(".said")?.textContent.trim().slice(0, 60) ?? "",
      ask: row.classList.contains("ask"),
    }));
    const out = document.createElement("pre");
    out.id = "rows";
    out.textContent = JSON.stringify(rows);
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>
`;

const dir = mkdtempSync(join(tmpdir(), "chat-ask-order-"));
const page = join(dir, "harness.html");
const html = spawnSync("cat", [join(SRC, "chat.html")], { encoding: "utf8" }).stdout
  .replace("<head>", `<head><base href="${pathToFileURL(SRC).href}">${stub}`);
writeFileSync(page, html);

const run = spawnSync(
  CHROME,
  [
    "--headless",
    "--disable-gpu",
    "--allow-file-access-from-files",
    "--virtual-time-budget=3000",
    "--dump-dom",
    pathToFileURL(page).href,
  ],
  { encoding: "utf8", maxBuffer: 1 << 24 },
);
const match = run.stdout.match(/<pre id="rows">(.*?)<\/pre>/s);
if (!match) {
  console.error("the harness never reported; chromium said:", run.stderr.slice(-2000));
  process.exit(2);
}
const rows = JSON.parse(match[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&"));
for (const [i, row] of rows.entries()) {
  console.log(`${i + 1}. ${row.ask ? "[ask] " : ""}${row.who}: ${row.said}`);
}
const askAt = rows.findIndex((row) => row.ask);
const answerAt = rows.findIndex((row) => row.said.startsWith(ANSWER.slice(0, 20)));
if (askAt === -1 || answerAt === -1) {
  console.error("expected both an ask row and the answer row");
  process.exit(2);
}
if (answerAt < askAt) {
  console.error(`FAIL: the answer (row ${answerAt + 1}) sits above the ask it followed (row ${askAt + 1})`);
  process.exit(1);
}
console.log("ok: the answer landed below the ask");
