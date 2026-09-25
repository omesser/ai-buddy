// A them-side reply that contains inline and fenced code is drawn by
// drawReply (the nodes are real) and still reads as prose, because the chip
// and the fence are painted with the quiet fills. On Terminal the fence fill
// is the panel. This drives the real Chat surface the way
// scripts/chat-ask-order.mjs does and asserts the paint a reader sees.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
const REPLY =
  "Run `cargo test -p ai-buddy-core` in the repo root.\n\n```bash\ncargo test -p ai-buddy-core -- --test-threads=1\n```";
const FENCE = "cargo test -p ai-buddy-core -- --test-threads=1";
const INLINE = "cargo test -p ai-buddy-core";

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

function channel(color) {
  const matched = String(color).match(
    /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\s/]+([\d.]+))?\)/,
  );
  assert.ok(matched, `unparsed color ${color}`);
  const alpha = matched[4] === undefined ? 1 : Number(matched[4]);
  return {
    r: Number(matched[1]),
    g: Number(matched[2]),
    b: Number(matched[3]),
    a: alpha,
  };
}

function over(fg, bg) {
  const a = fg.a;
  return {
    r: fg.r * a + bg.r * (1 - a),
    g: fg.g * a + bg.g * (1 - a),
    b: fg.b * a + bg.b * (1 - a),
  };
}

function apart(fg, bg) {
  const a = over(channel(fg), channel(bg));
  const b = channel(bg);
  return Math.hypot(a.r - b.r, a.g - b.g, a.b - b.b);
}

function paint(theme) {
  const opening = {
    name: "BMO",
    character: "Buddy Bot",
    configured: true,
    enabled: true,
    harness: { alive: true, missing: false, initializing: false },
    model: "",
    host: "",
    chat_ui: theme,
    login: null,
    harness_name: "Claude Code",
    instructions: "",
    personality: "",
    instance_prompt: "",
    prompt_limit: 2000,
  };
  const markdown = pathToFileURL(join(SRC, "markdown.js")).href;
  const stub = `
<script type="module">
  import { appendReply, drawReply } from ${JSON.stringify(markdown)};
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
  const full = ${JSON.stringify(REPLY)};
  const chunks = [
    "Run \`cargo",
    " test -p ai-buddy-core\` in the repo root.\\n\\n\`\`\`bash\\ncargo test",
    " -p ai-buddy-core -- --test-threads=1\\n\`\`\`",
  ];
  function paintOf(node) {
    const style = getComputedStyle(node);
    return {
      backgroundColor: style.backgroundColor,
      borderTopWidth: style.borderTopWidth,
      fontFamily: style.fontFamily,
    };
  }
  async function drive() {
    while (!handlers.chat || document.getElementById("line").disabled) await tick();
    document.getElementById("line").value = "how do I run the tests?";
    document.getElementById("composer").requestSubmit();
    while (!document.querySelector("#log .row.you")) await tick();
    handlers.chat({
      payload: {
        said: full,
        busy: false,
        reacting_to: null,
        you: false,
        at: null,
        error: null,
        superseded_by: null,
      },
    });
    await tick();
    const said = document.querySelector("#log .row.them .said");
    const one = document.createElement("div");
    drawReply(one, full);
    const streamed = document.createElement("div");
    for (const chunk of chunks) appendReply(streamed, chunk);
    const code = said.querySelector("p code");
    const pre = said.querySelector("pre");
    const fence = said.querySelector("pre code");
    const report = {
      scheme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
      saidClass: said.className,
      backticksLeft: said.textContent.includes("\`"),
      inline: code ? code.textContent : null,
      fence: fence ? fence.textContent : null,
      panel: getComputedStyle(document.body).backgroundColor,
      inlineCode: paintOf(code),
      pre: paintOf(pre),
      fenceCode: paintOf(fence),
      streamFence: streamed.querySelector("pre code")?.textContent ?? null,
      oneShotFence: one.querySelector("pre code")?.textContent ?? null,
      streamInline: streamed.querySelector("p code")?.textContent ?? null,
    };
    const out = document.createElement("pre");
    out.id = "probe";
    out.textContent = JSON.stringify(report);
    document.body.append(out);
  }
  window.addEventListener("load", drive);
</script>`;

  const dir = mkdtempSync(join(tmpdir(), "chat-reply-code-"));
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
      "--force-dark-mode",
      "--dump-dom",
      pathToFileURL(page).href,
    ],
    { encoding: "utf8", maxBuffer: 1 << 24, timeout: 40000, killSignal: "SIGKILL" },
  );
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(
    match,
    `the Chat surface did not report (${theme}). ${run.stderr?.slice(-500) ?? ""}`,
  );
  const json = match[1]
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
  return JSON.parse(json);
}

test(
  "a harness reply's inline and fenced code read apart from the panel",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 180000 },
  () => {
    for (const theme of ["minimal", "terminal"]) {
      const report = paint(theme);
      assert.equal(report.scheme, "dark", theme);
      assert.equal(report.saidClass, "said md", theme);
      assert.equal(report.backticksLeft, false, theme);
      assert.equal(report.inline, INLINE, theme);
      assert.equal(report.fence, FENCE, theme);
      assert.equal(report.streamInline, INLINE, `${theme} stream diverged from one shot`);
      assert.equal(report.streamFence, report.oneShotFence, `${theme} stream diverged from one shot`);
      assert.equal(report.streamFence, FENCE, theme);
      assert.equal(report.inlineCode.borderTopWidth, "1px", `${theme} inline has no edge`);
      assert.equal(report.pre.borderTopWidth, "1px", `${theme} fence has no edge`);
      assert.equal(report.fenceCode.borderTopWidth, "0px", `${theme} fence text is boxed twice`);
      assert.match(report.inlineCode.fontFamily, /mono/, theme);
      const inlineDelta = apart(report.inlineCode.backgroundColor, report.panel);
      const fenceDelta = apart(report.pre.backgroundColor, report.panel);
      assert.ok(
        inlineDelta >= 45,
        `${theme} inline code is ${inlineDelta.toFixed(1)} RGB steps from the panel`,
      );
      assert.ok(
        fenceDelta >= 32,
        `${theme} fence is ${fenceDelta.toFixed(1)} RGB steps from the panel`,
      );
    }
  },
);
