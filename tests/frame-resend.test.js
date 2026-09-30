// A resend of an unchanged placement asks for no display frame, and the first
// placement a late listener hears is still drawn.

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
      dialogue: null,
      thinking: false,
      asking: false,
      bubble: true,
      cue: null,
    },
  ],
  visible: true,
  fade_ms: 0,
  sound: false,
};

const nextFrame = {
  ...frame,
  sprites: [{ ...frame.sprites[0], frame_index: 1 }],
};

function run() {
  const stub = `
<script type="module">
  const handlers = {};
  // Headless Chromium under virtual time never runs a frame callback, so the
  // test is the display: it counts what was asked for and runs it.
  let asks = 0;
  let queued = [];
  window.requestAnimationFrame = (fn) => {
    asks += 1;
    queued.push(fn);
    return asks;
  };
  const paint = () => {
    const due = queued;
    queued = [];
    for (const fn of due) fn(performance.now());
  };
  window.__TAURI__ = {
    core: {
      invoke(name) {
        if (name === "character") return Promise.resolve({ characters: {} });
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
  // Runs every frame asked for until none is pending, so the count that
  // follows is the listener's own and not a frame something else still holds.
  const settle = async () => {
    await tick();
    while (queued.length) {
      paint();
      await tick();
    }
  };
  const deliver = async (payload) => {
    await settle();
    const before = asks;
    handlers.frame({ payload });
    const asked = asks - before;
    await settle();
    return asked;
  };
  async function drive() {
    while (!handlers.frame) await tick();
    const first = await deliver(${JSON.stringify(frame)});
    const sprite = document.querySelector('.sprite[data-instance="bmo"]');
    const drawn = {
      visibility: sprite.style.visibility,
      transform: sprite.style.transform,
      animation: sprite.dataset.animation,
      frameIndex: sprite.dataset.frameIndex,
    };
    const resend = await deliver(${JSON.stringify(frame)});
    const changed = await deliver(${JSON.stringify(nextFrame)});
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify({
      asks: { first, resend, changed },
      drawn,
      frameIndex: sprite.dataset.frameIndex,
    });
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "frame-resend-"));
  const page = join(dir, "harness.html");
  const html = readFileSync(join(SRC, "index.html"), "utf8").replace(
    "<head>",
    `<head><base href="${pathToFileURL(SRC).href}">${stub}`,
  );
  writeFileSync(page, html);

  const result = spawnSync(
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
  const match = result.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the overlay did not report. ${result.stderr?.slice(-500) ?? ""}`);
  return JSON.parse(
    match[1]
      .replace(/&quot;/g, '"')
      .replace(/&amp;/g, "&")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">"),
  );
}

const skip = chrome ? false : "headless Chromium is not installed";
let report = null;
const probe = () => (report ??= run());

test("a late listener's first placement is drawn", { skip, timeout: 60000 }, () => {
  const { asks, drawn } = probe();
  assert.equal(asks.first, 1);
  assert.deepEqual(drawn, {
    visibility: "visible",
    transform: "translate(200px, 200px) scaleX(1)",
    animation: "idle",
    frameIndex: "0",
  });
});

test("an unchanged resend asks for no frame, and a change still does", { skip, timeout: 60000 }, () => {
  const { asks, frameIndex } = probe();
  assert.equal(asks.resend, 0, "the resend requested a display frame");
  assert.equal(asks.changed, 1);
  assert.equal(frameIndex, "1");
});
