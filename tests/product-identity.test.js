import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { createNamesNotice } from "../src/chat-names-hint.js";

test("window titles use the product name Fidget", () => {
  for (const file of ["src/index.html", "src/chat.html", "src/settings.html"]) {
    const html = readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
    const title = html.match(/<title>([^<]*)<\/title>/)[1];
    assert.match(title, /^Fidget\b/, file);
  }
});

test("the bundle identifier and product name are Fidget", () => {
  const conf = JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(conf.productName, "Fidget");
  assert.equal(conf.identifier, "dev.omesser.fidget");
  assert.equal(conf.plugins.updater.endpoints[0].includes("github.com/omesser/ai-buddy/"), true);
});

test("the names notice calls the companion a character", () => {
  const notice = createNamesNotice({ act: async () => "quiet" });
  const { view } = notice.receive({ hint: "due", generation: 1 });
  assert.equal(
    view.body,
    "The character knows where your windows are, not what they are. One switch in Settings turns on titles and application names together.",
  );
});

test("development switches are FIDGET_ variables", () => {
  const src = readFileSync(new URL("../src-tauri/src/dev_flags.rs", import.meta.url), "utf8");
  assert.match(src, /Flag::new\("FIDGET_TRACE_FRAMES"\)/);
  assert.equal(src.includes("AI_" + "BUDDY"), false);
});
