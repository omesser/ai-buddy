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

test("chat-ui.css has no prefers-color-scheme", () => {
  assert.doesNotMatch(CSS, /prefers-color-scheme/);
});

test("each design has a light palette block, and glass covers of and what", () => {
  for (const design of DESIGNS) {
    assert.match(
      CSS,
      new RegExp(String.raw`\[data-chat-palette="light"\]\.chat-ui-${design}\b`),
      design,
    );
  }
  assert.match(
    CSS,
    /\[data-chat-palette="light"\]\.chat-ui-glass \.tb \.of\s*\{[^}]*color:\s*rgba\(26,\s*31,\s*43,\s*\.62\)/s,
  );
  assert.match(
    CSS,
    /\[data-chat-palette="light"\]\.chat-ui-glass \.ask \.what\s*\{[^}]*color:\s*rgba\(26,\s*31,\s*43,\s*\.82\)/s,
  );
});

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

function probe(script, forceDark) {
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
      ${script}
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

const REPORT = `
function report(extra) {
  const html = document.documentElement;
  const style = getComputedStyle(html);
  const out = document.createElement("pre");
  out.id = "probe";
  out.textContent = JSON.stringify({
    panel: style.getPropertyValue("--chat-panel").trim(),
    colorScheme: style.colorScheme,
    palette: html.dataset.chatPalette,
    className: html.className,
    ...extra,
  });
  document.body.append(out);
}
`;

test(
  "forced light paints each design's light panel and pins color-scheme",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 180000 },
  () => {
    for (const design of DESIGNS) {
      const expected = declaredToken(`[data-chat-palette="light"].chat-ui-${design}`, "--chat-panel");
      const report = probe(
        `
        ${REPORT}
        const html = document.documentElement;
        html.className = ${JSON.stringify(`chat-ui-${design}`)};
        const apply = mountChatAppearance(html);
        apply("light");
        report();
      `,
        true,
      );
      assert.equal(report.colorScheme, "light", design);
      assert.equal(report.panel.replace(/\s+/g, ""), expected, design);
    }
  },
);

test(
  "system and dark under force-dark-mode keep the dark panel",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 180000 },
  () => {
    for (const design of DESIGNS) {
      const expected = declaredToken(`.chat-ui-${design}`, "--chat-panel");
      const dark = probe(
        `
        ${REPORT}
        const html = document.documentElement;
        html.className = ${JSON.stringify(`chat-ui-${design}`)};
        const apply = mountChatAppearance(html);
        apply("dark");
        report();
      `,
        true,
      );
      assert.equal(dark.colorScheme, "dark", design);
      assert.equal(dark.panel.replace(/\s+/g, ""), expected, `${design} dark`);

      const system = probe(
        `
        ${REPORT}
        const html = document.documentElement;
        html.className = ${JSON.stringify(`chat-ui-${design}`)};
        const apply = mountChatAppearance(html);
        apply("system");
        report({ after: html.dataset.chatPalette });
      `,
        true,
      );
      assert.equal(system.palette, "dark", `${design} system`);
      assert.equal(system.panel.replace(/\s+/g, ""), expected, `${design} system panel`);
    }
  },
);

test(
  "changing data-chat-palette from dark to light repaints the panel",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 180000 },
  () => {
    const light = declaredToken(`[data-chat-palette="light"].chat-ui-minimal`, "--chat-panel");
    const dark = declaredToken(`.chat-ui-minimal`, "--chat-panel");
    const report = probe(
      `
      ${REPORT}
      const html = document.documentElement;
      html.className = "chat-ui-minimal";
      const apply = mountChatAppearance(html);
      apply("dark");
      const before = getComputedStyle(html).getPropertyValue("--chat-panel").trim();
      apply("light");
      report({ before });
    `,
      true,
    );
    assert.equal(report.before.replace(/\s+/g, ""), dark);
    assert.equal(report.panel.replace(/\s+/g, ""), light);
    assert.equal(report.colorScheme, "light");
  },
);

test(
  "dark forces the dark panel when the browser is not in force-dark-mode",
  { skip: chrome ? false : "headless Chromium is not installed", timeout: 180000 },
  () => {
    for (const design of DESIGNS) {
      const dark = declaredToken(`.chat-ui-${design}`, "--chat-panel");
      const light = declaredToken(`[data-chat-palette="light"].chat-ui-${design}`, "--chat-panel");
      const report = probe(
        `
        ${REPORT}
        const html = document.documentElement;
        html.className = ${JSON.stringify(`chat-ui-${design}`)};
        const apply = mountChatAppearance(html);
        apply("system");
        const systemPalette = html.dataset.chatPalette;
        const systemPanel = getComputedStyle(html).getPropertyValue("--chat-panel").trim();
        apply("dark");
        const darkPanel = getComputedStyle(html).getPropertyValue("--chat-panel").trim();
        const darkScheme = getComputedStyle(html).colorScheme;
        apply("light");
        report({ systemPalette, systemPanel, darkPanel, darkScheme });
      `,
        false,
      );
      assert.equal(report.systemPalette, "light", `${design} system`);
      assert.equal(report.systemPanel.replace(/\s+/g, ""), light, `${design} system panel`);
      assert.equal(report.darkPanel.replace(/\s+/g, ""), dark, `${design} dark panel`);
      assert.equal(report.darkScheme, "dark", `${design} dark scheme`);
      assert.equal(report.panel.replace(/\s+/g, ""), light, `${design} light panel`);
      assert.equal(report.colorScheme, "light", `${design} light scheme`);
    }
  },
);
