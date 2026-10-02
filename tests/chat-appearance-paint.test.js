import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));
const CSS = readFileSync(new URL("../src/chat-ui.css", import.meta.url), "utf8");
const DESIGNS = ["minimal", "terminal", "glass"];

function declaredToken(selector, name) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const block = CSS.match(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`));
  assert.ok(block, `no block for ${selector}`);
  const decl = block[1].match(new RegExp(`${name}\\s*:\\s*([^;]+);`));
  assert.ok(decl, `${selector} has no ${name}`);
  return resolveToken(decl[1].trim());
}

function resolveToken(value) {
  const compact = value.replace(/\s+/g, "");
  const ref = compact.match(/^var\((--[A-Za-z0-9-]+)\)$/);
  if (!ref) {
    return compact;
  }
  const snippet = readFileSync(new URL("../src/chat-shared.css", import.meta.url), "utf8");
  const decl = snippet.match(new RegExp(`${ref[1]}\\s*:\\s*([^;]+);`));
  assert.ok(decl, `${ref[1]} is not in chat-shared.css`);
  return decl[1].trim().replace(/\s+/g, "");
}

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

// One Chrome launch per test: `body` runs once per design on the same page and
// returns that design's readings. A launch costs seconds, the loop costs nothing.
function probe(body, forceDark) {
  const dir = mkdtempSync(join(tmpdir(), "chat-appearance-paint-"));
  const page = join(dir, "harness.html");
  const appearance = pathToFileURL(join(SRC, "chat-appearance.js")).href;
  const html = `<!doctype html>
<html>
  <head>
    <base href="${pathToFileURL(SRC).href}">
    <link rel="stylesheet" href="chat-ui.css">
  </head>
  <body>
    <script type="module">
      import { mountChatAppearance } from ${JSON.stringify(appearance)};
      const html = document.documentElement;
      const panel = () => getComputedStyle(html).getPropertyValue("--chat-panel").trim();
      const read = (extra) => ({
        panel: panel(),
        colorScheme: getComputedStyle(html).colorScheme,
        palette: html.dataset.chatPalette,
        ...extra,
      });
      const out = {};
      for (const design of ${JSON.stringify(DESIGNS)}) {
        html.className = "chat-ui-" + design;
        out[design] = (() => {
          ${body}
        })();
      }
      const pre = document.createElement("pre");
      pre.id = "probe";
      pre.textContent = JSON.stringify(out);
      document.body.append(pre);
    </script>
  </body>
</html>`;
  writeFileSync(page, html);
  const args = [
    "--headless",
    "--disable-gpu",
    "--no-sandbox",
    "--disable-dev-shm-usage",
    `--user-data-dir=${join(dir, "profile")}`,
    "--allow-file-access-from-files",
    "--virtual-time-budget=3000",
  ];
  if (forceDark) {
    args.push("--force-dark-mode");
  }
  args.push("--dump-dom", pathToFileURL(page).href);
  const run = spawnSync(chrome, args, {
    encoding: "utf8",
    maxBuffer: 1 << 24,
    timeout: 40000,
    killSignal: "SIGKILL",
  });
  const match = run.stdout.match(/<pre id="probe"[^>]*>(.*?)<\/pre>/s);
  assert.ok(match, `the page did not report. ${run.stderr?.slice(-500) ?? ""}`);
  return JSON.parse(
    match[1]
      .replace(/&quot;/g, '"')
      .replace(/&amp;/g, "&")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">"),
  );
}

const flat = (value) => value.replace(/\s+/g, "");
const browser = { skip: chrome ? false : "headless Chromium is not installed", timeout: 60000 };

test("forced light paints each design's light panel and pins color-scheme", browser, () => {
  const reports = probe(
    `
    mountChatAppearance(html)("light");
    return read();
  `,
    true,
  );
  for (const design of DESIGNS) {
    const expected = declaredToken(`[data-chat-palette="light"].chat-ui-${design}`, "--chat-panel");
    assert.equal(reports[design].colorScheme, "light", design);
    assert.equal(flat(reports[design].panel), expected, design);
  }
});

test("system and dark under force-dark-mode keep the dark panel", browser, () => {
  const reports = probe(
    `
    mountChatAppearance(html)("dark");
    const dark = read();
    mountChatAppearance(html)("system");
    return { dark, system: read() };
  `,
    true,
  );
  for (const design of DESIGNS) {
    const expected = declaredToken(`.chat-ui-${design}`, "--chat-panel");
    const { dark, system } = reports[design];
    assert.equal(dark.colorScheme, "dark", design);
    assert.equal(flat(dark.panel), expected, `${design} dark`);
    assert.equal(system.palette, "dark", `${design} system`);
    assert.equal(flat(system.panel), expected, `${design} system panel`);
  }
});

test("changing data-chat-palette from dark to light repaints the panel", browser, () => {
  const reports = probe(
    `
    const apply = mountChatAppearance(html);
    apply("dark");
    const before = panel();
    apply("light");
    return read({ before });
  `,
    true,
  );
  for (const design of DESIGNS) {
    const report = reports[design];
    assert.equal(flat(report.before), declaredToken(`.chat-ui-${design}`, "--chat-panel"), design);
    assert.equal(
      flat(report.panel),
      declaredToken(`[data-chat-palette="light"].chat-ui-${design}`, "--chat-panel"),
      design,
    );
    assert.equal(report.colorScheme, "light", design);
  }
});

test("dark forces the dark panel when the browser is not in force-dark-mode", browser, () => {
  const reports = probe(
    `
    const apply = mountChatAppearance(html);
    apply("system");
    const systemPalette = html.dataset.chatPalette;
    const systemPanel = panel();
    apply("dark");
    const darkPanel = panel();
    const darkScheme = getComputedStyle(html).colorScheme;
    apply("light");
    return read({ systemPalette, systemPanel, darkPanel, darkScheme });
  `,
    false,
  );
  for (const design of DESIGNS) {
    const dark = declaredToken(`.chat-ui-${design}`, "--chat-panel");
    const light = declaredToken(`[data-chat-palette="light"].chat-ui-${design}`, "--chat-panel");
    const report = reports[design];
    assert.equal(report.systemPalette, "light", `${design} system`);
    assert.equal(flat(report.systemPanel), light, `${design} system panel`);
    assert.equal(flat(report.darkPanel), dark, `${design} dark panel`);
    assert.equal(report.darkScheme, "dark", `${design} dark scheme`);
    assert.equal(flat(report.panel), light, `${design} light panel`);
    assert.equal(report.colorScheme, "light", `${design} light scheme`);
  }
});
