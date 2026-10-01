// A branded connect leaves the landing status at the top of the log.
// Drives src/chat.html.

import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));

function chromeBin() {
  if (process.env.FIDGET_CHROME) return process.env.FIDGET_CHROME;
  for (const name of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) {
    const found = spawnSync("bash", ["-lc", `command -v ${name}`], { encoding: "utf8" });
    const path = found.stdout.trim();
    if (found.status === 0 && path) return path;
  }
  return null;
}

const chrome = chromeBin();
const skip = chrome ? false : "headless Chromium is not installed";

const unconfigured = {
  name: "BMO",
  character: "Buddy Bot",
  configured: false,
  enabled: true,
  harness: null,
  model: "",
  host: "",
  chat_ui: "minimal",
  login: null,
  harness_name: null,
  instructions: "",
  personality: "",
  instance_prompt: "",
  prompt_limit: 2000,
};

function opening(harness) {
  return {
    ...unconfigured,
    configured: true,
    harness_name: "grok",
    harness: { name: "grok", session: null, alive: false, login: null, ...harness },
  };
}

const initializing = opening({ initializing: true });
const ready = opening({ alive: true, session: "abcdef0123456789" });

function freePort() {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function withChat(run, { rejectHarness = false } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "chat-connect-scroll-"));
  const page = join(dir, "harness.html");
  const stub = `
<script type="module">
  const handlers = {};
  window.__handlers = handlers;
  window.__invoked = [];
  window.__TAURI__ = {
    core: {
      invoke(name, args) {
        window.__invoked.push({ name, args });
        if (name === "chat_opening") return Promise.resolve(${JSON.stringify(unconfigured)});
        if (name === "select_harness") {
          return ${rejectHarness ? 'Promise.reject("unknown Harness preset: grok")' : 'Promise.resolve("ok")'};
        }
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
  window.__snap = () => {
    const log = document.getElementById("log");
    const title = document.getElementById("landing-title");
    const active = document.activeElement;
    const logBox = log.getBoundingClientRect();
    const titleBox = title.getBoundingClientRect();
    const note = log.querySelector(".note:last-of-type");
    const noteBox = note?.getBoundingClientRect();
    return {
      scrollTop: log.scrollTop,
      title: title.textContent,
      titleInView: titleBox.top >= logBox.top - 1 && titleBox.top < logBox.bottom,
      active: active?.dataset?.harness || active?.id || active?.tagName,
      invoked: window.__invoked.map((call) => call.name),
      note: note?.textContent ?? "",
      noteInView: Boolean(noteBox && noteBox.top >= logBox.top - 1 && noteBox.bottom <= logBox.bottom + 1),
    };
  };
  window.__reveal = (harness) => {
    const log = document.getElementById("log");
    const btn = document.querySelector(\`.connect-btn[data-harness="\${harness}"]\`);
    log.scrollTop = 0;
    const logBox = log.getBoundingClientRect();
    const btnBox = btn.getBoundingClientRect();
    log.scrollTop = Math.max(0, btnBox.top - logBox.top - 24);
    const placed = btn.getBoundingClientRect();
    return {
      x: placed.left + placed.width / 2,
      y: placed.top + placed.height / 2,
      scrollTop: log.scrollTop,
    };
  };
</script>`;
  const html = spawnSync("cat", [join(SRC, "chat.html")], { encoding: "utf8" }).stdout.replace(
    "<head>",
    `<head><base href="${pathToFileURL(SRC).href}">${stub}`,
  );
  writeFileSync(page, html);

  const port = await freePort();
  const child = spawn(
    chrome,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      `--user-data-dir=${join(dir, "profile")}`,
      "--allow-file-access-from-files",
      `--remote-debugging-port=${port}`,
      "--window-size=420,560",
      pathToFileURL(page).href,
    ],
    { stdio: "ignore" },
  );

  try {
    let wsUrl = null;
    const href = pathToFileURL(page).href;
    for (let i = 0; i < 50 && !wsUrl; i++) {
      try {
        const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
        wsUrl = pages.find((item) => item.type === "page" && item.url === href)?.webSocketDebuggerUrl;
      } catch {}
      if (!wsUrl) await sleep(100);
    }
    assert.ok(wsUrl, "Chrome did not open a debuggable page");

    const ws = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve);
      ws.addEventListener("error", reject);
    });
    let seq = 0;
    const pending = new Map();
    ws.addEventListener("message", (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id && pending.has(msg.id)) {
        pending.get(msg.id)(msg);
        pending.delete(msg.id);
      }
    });
    const send = (method, params = {}) =>
      new Promise((resolve) => {
        const id = ++seq;
        pending.set(id, resolve);
        ws.send(JSON.stringify({ id, method, params }));
      });
    const evalJs = async (expression) => {
      const msg = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
      assert.equal(msg.result?.exceptionDetails, undefined, JSON.stringify(msg.result?.exceptionDetails));
      return msg.result?.result?.value;
    };

    // A navigation swaps the document under a long-lived wait, so poll instead.
    const ready = `!!(window.__reveal && window.__handlers?.["chat-opening"] && document.getElementById("empty")?.hidden === false)`;
    let loaded = false;
    for (let i = 0; i < 100 && !loaded; i++) {
      const msg = await send("Runtime.evaluate", { expression: ready, returnByValue: true });
      loaded = msg.result?.result?.value === true;
      if (!loaded) await sleep(50);
    }
    assert.ok(loaded, "chat.html never finished loading");

    try {
      await run({ send, evalJs });
    } finally {
      ws.close();
    }
  } finally {
    child.kill("SIGKILL");
  }
}

test("a branded connect keeps the landing status at the top", { skip, timeout: 60000 }, async () => {
  await withChat(async ({ send, evalJs }) => {
    const place = await evalJs(`window.__reveal("grok")`);
    assert.ok(place.scrollTop > 0, "the transcript could not scroll");

    await send("Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: place.x,
      y: place.y,
      button: "left",
      clickCount: 1,
    });
    await send("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: place.x,
      y: place.y,
      button: "left",
      clickCount: 1,
    });

    const clicked = await evalJs("window.__snap()");
    assert.ok(clicked.invoked.includes("select_harness"), "the click missed the branded button");
    assert.equal(clicked.scrollTop, 0);
    assert.equal(clicked.active, "BODY");
    assert.equal(clicked.titleInView, true);

    await evalJs(`window.__handlers["chat-opening"]({ payload: ${JSON.stringify(initializing)} })`);
    await evalJs(`window.__handlers["chat-session"]({ payload: "settings changed what answers" })`);
    const aftermath = await evalJs("window.__snap()");
    assert.equal(aftermath.title, "Initializing Grok…");
    assert.equal(aftermath.scrollTop, 0);
    assert.equal(aftermath.titleInView, true);
    assert.equal(aftermath.active, "BODY");

    await evalJs(`window.__handlers["chat-opening"]({ payload: ${JSON.stringify(ready)} })`);
    const long = `${"line\n".repeat(40)}end`;
    await evalJs(`window.__handlers.chat({ payload: { you: true, said: ${JSON.stringify(long)} } })`);
    const followed = await evalJs("window.__snap()");
    assert.ok(followed.scrollTop > 0, `a later line stayed pinned at ${followed.scrollTop}`);
  });
});

test("a branded connect the Shell refuses shows its error", { skip, timeout: 60000 }, async () => {
  await withChat(
    async ({ send, evalJs }) => {
      const place = await evalJs(`window.__reveal("grok")`);
      await send("Input.dispatchMouseEvent", {
        type: "mousePressed",
        x: place.x,
        y: place.y,
        button: "left",
        clickCount: 1,
      });
      await send("Input.dispatchMouseEvent", {
        type: "mouseReleased",
        x: place.x,
        y: place.y,
        button: "left",
        clickCount: 1,
      });
      await evalJs("new Promise((resolve) => setTimeout(resolve, 0))");
      const failed = await evalJs("window.__snap()");
      assert.match(failed.note, /Could not connect to Grok: unknown Harness preset: grok/);
      assert.equal(failed.noteInView, true);
    },
    { rejectHarness: true },
  );
});
