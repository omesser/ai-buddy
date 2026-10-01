// The landing page is Generated (ADR-0038): its words come from README.md and
// its cast from characters/. These tests assemble the site the way pages.yml
// does, so a page that names art the site lacks fails here, not after deploy.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SCRIPT = join(ROOT, "scripts", "make-landing-page.py");
const PAGE = "landing.html";

function assembleScript(site) {
  const workflow = readFileSync(join(ROOT, ".github", "workflows", "pages.yml"), "utf8");
  const block = workflow.match(/- name: Assemble the site\n(?:\s+#.*\n)*\s+run: \|\n((?:\s{10}.*\n)+)/);
  assert.ok(block, "pages.yml has an Assemble the site step");
  return block[1].replaceAll("_site", site);
}

let scratch;
let site;

before(() => {
  scratch = mkdtempSync(join(tmpdir(), "landing-page-"));
  site = join(scratch, "_site");
  execFileSync("bash", ["-ec", assembleScript(site)], { cwd: ROOT, stdio: "pipe" });
});

after(() => rmSync(scratch, { recursive: true, force: true }));

const published = (name) => readFileSync(join(site, name), "utf8");

test("the generator self-check passes", () => {
  const out = execFileSync("python3", [SCRIPT, "--self-check"], { cwd: ROOT, encoding: "utf8" });
  assert.match(out, /self-check:/);
});

test("the page leads with the README headline", () => {
  const readme = readFileSync(join(ROOT, "README.md"), "utf8");
  const headline = readme.match(/^# (.+)$/m)[1];
  assert.match(published(PAGE), new RegExp(`<h1>${headline.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}</h1>`));
});

test("the page names every Character a manifest declares", () => {
  const page = published(PAGE);
  const names = readdirSync(join(ROOT, "characters"))
    .filter((dir) => existsSync(join(ROOT, "characters", dir, "character.manifest")))
    .map((dir) =>
      readFileSync(join(ROOT, "characters", dir, "character.manifest"), "utf8").match(/^name = "(.+)"$/m)[1],
    );
  assert.ok(names.length > 1, "characters/ holds the cast");
  for (const name of names) {
    assert.ok(page.includes(`<figcaption>${name}</figcaption>`), `the cast shows ${name}`);
  }
});

test("every frame the page names is in the assembled site", () => {
  const page = published(PAGE);
  const srcs = [...page.matchAll(/src="([^"]+)"/g)].map((m) => m[1]);
  const loops = [...page.matchAll(/data-frames="([^"]+)"/g)].flatMap((m) =>
    JSON.parse(m[1].replaceAll("&quot;", '"')),
  );
  assert.ok(loops.length > srcs.length, "the cast and the hero carry their loops");
  for (const path of [...srcs, ...loops].filter((p) => !p.startsWith("http"))) {
    assert.ok(existsSync(join(site, path)), `${path} is published`);
  }
});

test("a fixture README reaches the page, and one with no H1 fails the build", () => {
  const readme = join(scratch, "README.md");
  const out = join(scratch, "fixture");
  const rest = readFileSync(join(ROOT, "README.md"), "utf8").replace(/^# .+$/m, "");

  writeFileSync(readme, `# A fixture headline\n${rest}`);
  execFileSync("python3", [SCRIPT, "--readme", readme, "--out", out], { cwd: ROOT, stdio: "pipe" });
  assert.match(readFileSync(join(out, PAGE), "utf8"), /<h1>A fixture headline<\/h1>/);

  writeFileSync(readme, rest);
  assert.throws(
    () => execFileSync("python3", [SCRIPT, "--readme", readme, "--out", out], { cwd: ROOT, stdio: "pipe" }),
    (error) => /no H1/.test(error.stderr.toString()),
  );
});
