// The Chat surface itself, driven headless the way chat-header-wrap.test.js
// drives it: a typed turn, a thought streaming into the log, the reply landing.
// The rule is chat-thinking.test.js; this checks the rows it draws, and where.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));

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

const OPENING = {
  name: "Buddy Bot",
  character: "Buddy Bot",
  configured: true,
  enabled: true,
  harness: { name: "claude", session: "fd4be1a2", alive: true, missing: false, initializing: false, login: null },
  model: "",
  host: "",
  chat_ui: "minimal",
  login: null,
  harness_name: "claude",
  instructions: "",
  personality: "",
  instance_prompt: "",
  prompt_limit: 2000,
};

function drive() {
  const stub = `
<script>
  const heard = {};
  window.__TAURI__ = {
    core: {
      invoke(name) {
        if (name === "chat_opening") return Promise.resolve(${JSON.stringify(OPENING)});
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
  const emit = (name, payload) => heard[name]({ payload });
  function rows() {
    return [...document.querySelectorAll("#log > .row, #log > .note")].map((row) => {
      const body = row.querySelector(".said");
      const toggle = row.querySelector(".thinking-toggle");
      return {
        kind: row.classList.contains("thinking") ? "thinking" : row.classList.contains("you") ? "you" : row.classList.contains("them") ? "them" : "note",
        label: row.querySelector(".who-label")?.textContent ?? null,
        stamped: Boolean(row.querySelector("time.when")?.dateTime),
        text: body?.textContent ?? row.textContent,
        open: body ? !body.hidden : null,
        expanded: toggle ? toggle.getAttribute("aria-expanded") : null,
      };
    });
  }
  async function run() {
    while (!heard["chat-opening"] || !heard["chat-thought"]) await tick();
    while (document.getElementById("line").disabled) await tick();
    const report = { strip: Boolean(document.getElementById("thought")) };
    document.getElementById("line").value = "what are you standing on?";
    document.getElementById("composer").requestSubmit();
    while (!document.querySelector("#log > .row.them")) await tick();
    emit("chat-thought", "Reading the roster");
    emit("chat-thought", "Reading the roster\\nChecking the desk");
    report.streaming = rows();
    emit("chat-thought", "");
    emit("chat", { said: "The desktop floor.", busy: false, reacting_to: null, you: false, at: null, error: null });
    report.landed = rows();
    document.querySelector(".row.thinking .thinking-toggle").click();
    report.expanded = rows();
    emit("chat", { said: "Kept from before", thought: true, busy: false, reacting_to: null, you: false, at: 1700000000000, error: null });
    report.kept = rows().at(-1);
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

  const dir = mkdtempSync(join(tmpdir(), "chat-thinking-render-"));
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
  assert.ok(match, `the Chat surface did not report. ${run.stderr?.slice(-500) ?? ""}`);
  return JSON.parse(match[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">"));
}

test(
  "thinking streams open between the question and the answer, and the answer collapses it",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 60000 },
  () => {
    const report = drive();
    assert.equal(report.error, undefined, report.error);
    assert.equal(report.strip, false, "the strip above the composer is gone");

    const thinking = (text, open, expanded) => ({
      kind: "thinking",
      label: "Thinking",
      stamped: true,
      text,
      open,
      expanded,
    });
    const you = { kind: "you", label: "You", stamped: true, text: "what are you standing on?", open: true, expanded: null };
    const said = "Reading the roster\nChecking the desk";

    assert.deepEqual(report.streaming.map((row) => row.kind), ["you", "thinking", "them"]);
    assert.deepEqual(report.streaming.slice(0, 2), [you, thinking(said, true, "true")]);

    assert.deepEqual(report.landed, [
      you,
      thinking(said, false, "false"),
      { kind: "them", label: "Buddy Bot", stamped: true, text: "The desktop floor.", open: true, expanded: null },
    ]);

    assert.deepEqual(report.expanded[1], thinking(said, true, "true"));
    assert.deepEqual(report.kept, thinking("Kept from before", false, "false"));
  },
);
