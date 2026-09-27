import assert from "node:assert/strict";
import { test } from "node:test";

import { classifyLine, failingHits, transform, transformPath } from "../scripts/rename-to-fidget.mjs";

test("forge coordinates stay the repo slug", () => {
  const url = "https://github.com/omesser/ai-buddy/releases/latest/download/latest.json";
  assert.equal(transform(url), url);
  assert.equal(transform("git clone https://github.com/omesser/ai-buddy.git"), "git clone https://github.com/omesser/ai-buddy.git");
  assert.equal(transform("See omesser/ai-buddy#37."), "See omesser/ai-buddy#37.");
  assert.equal(
    transform("Try https://omesser.github.io/ai-buddy/cues.html"),
    "Try https://omesser.github.io/ai-buddy/cues.html",
  );
});

test("product identifiers split display and slug", () => {
  assert.equal(transform("AI_BUDDY_DIRECTOR"), "FIDGET_DIRECTOR");
  assert.equal(transform("ai_buddy_core::memory"), "fidget_core::memory");
  assert.equal(transform("ai-buddy-core"), "fidget-core");
  assert.equal(transform("name = \"ai-buddy\""), "name = \"fidget\"");
  assert.equal(transform('"productName": "ai-buddy"'), '"productName": "Fidget"');
  assert.equal(transform("<title>ai-buddy Settings</title>"), "<title>Fidget Settings</title>");
  assert.equal(transform('.tooltip("ai-buddy")'), '.tooltip("Fidget")');
  assert.equal(transform("dev.omesser.ai-buddy"), "dev.omesser.fidget");
  assert.equal(transform("/Applications/ai-buddy.app/Contents/MacOS/ai-buddy"), "/Applications/Fidget.app/Contents/MacOS/fidget");
  assert.equal(transform("lets ai-buddy read titles"), "lets Fidget read titles");
  assert.equal(transform('Some("ai-buddy holds no credential")'), 'Some("Fidget holds no credential")');
  assert.equal(transform('"Point a Harness at ai-buddy"'), '"Point a Harness at Fidget"');
  assert.equal(transform("under ai-buddy's conditions"), "under Fidget's conditions");
  assert.equal(transform('description = "ai-buddy\'s pure core"'), 'description = "Fidget\'s pure core"');
  assert.equal(transform('.title("ai-buddy")'), '.title("Fidget")');
  assert.equal(transform("[behaviors.fidget]"), "[behaviors.fidget]");
  assert.equal(transform("xdotool search --class 'Ai-buddy'"), "xdotool search --class 'Fidget'");
  assert.equal(transform("exposing buddy-side tools"), "exposing Fidget-side tools");
  assert.equal(transform("multi-buddy in one process"), "multi-character in one process");
  assert.equal(transform('asking_as("buddy-1", "bmo")'), 'asking_as("buddy-1", "bmo")');
  assert.equal(transform('("fidget", 2, None)'), '("fidget", 2, None)');
  assert.equal(transform("//! ai-buddy's overlay shell."), "//! Fidget's overlay shell.");
  assert.equal(
    transform('https:\\/\\/github\\.com\\/omesser\\/ai-buddy\\/issues\\/277'),
    'https:\\/\\/github\\.com\\/omesser\\/ai-buddy\\/issues\\/277',
  );
  assert.equal(transform("not an ai-buddy checkout"), "not a Fidget checkout");
  assert.equal(transform('"ai-buddy".into()'), '"Fidget".into()');
  assert.equal(transform("target/debug/ai-buddy"), "target/debug/fidget");
  assert.equal(transform("cargo test -p ai-buddy --bin ai-buddy"), "cargo test -p fidget --bin fidget");
  assert.equal(transform("pgrep -x ai-buddy"), "pgrep -x fidget");
  assert.equal(transform("claude mcp add ai-buddy --url"), "claude mcp add fidget --url");
  assert.equal(transform("claude mcp remove ai-buddy 2>/dev/null"), "claude mcp remove fidget 2>/dev/null");
  assert.equal(transform("ai-buddy://windows"), "fidget://windows");
  assert.equal(transform('"a ai-buddy window"'), '"a fidget window"');
  assert.equal(transform("pass:ai-buddy"), "pass:fidget");
  assert.equal(transform("not by ai-buddy."), "not by Fidget.");
  assert.equal(transform("outside ai-buddy: in the harness"), "outside Fidget: in the harness");
  assert.equal(transform("  ai-buddy:\n    url: x"), "  fidget:\n    url: x");
  assert.equal(transform("target/debug/ai-buddy.exe"), "target/debug/fidget.exe");
  assert.equal(transform("a packaged build is ai-buddy."), "a packaged build is Fidget.");
  assert.equal(
    transform('const KEY = "ai-buddy.window-names-hint.dismissed"'),
    'const KEY = "fidget.window-names-hint.dismissed"',
  );
});

test("the companion word becomes character and named packages stay", () => {
  assert.equal(
    transform("The buddy knows where your windows are. How many buddies are running."),
    "The character knows where your windows are. How many characters are running.",
  );
  assert.equal(transform("Buddy Bot uses characters/buddy-bot"), "Buddy Bot uses characters/buddy-bot");
  assert.equal(transform("/Users/buddy/Library/Application Support/ai-buddy"), "/Users/buddy/Library/Application Support/fidget");
  assert.equal(transform('post(&url, "hello", "buddy-1")'), 'post(&url, "hello", "buddy-1")');
  assert.equal(transform("Buddy Cues"), "Fidget Cues");
  assert.equal(
    transform("_Avoid_: Pet, mascot, avatar, buddy (the app is the buddy, not the character)"),
    "_Avoid_: Pet, mascot, avatar, Fidget (Fidget is the product, not a Character)",
  );
});

test("paths rename product slugs and keep the character package", () => {
  assert.equal(transformPath(".agents/skills/verify-ai-buddy/SKILL.md"), ".agents/skills/verify-fidget/SKILL.md");
  assert.equal(
    transformPath("docs/adr/0027-ai-buddy-verify-is-the-agent-entry.md"),
    "docs/adr/0027-fidget-verify-is-the-agent-entry.md",
  );
  assert.equal(
    transformPath("docs/research/buddy-harness-two-way.md"),
    "docs/research/fidget-harness-two-way.md",
  );
  assert.equal(transformPath("characters/buddy-bot/character.manifest"), "characters/buddy-bot/character.manifest");
});

test("a transformed line has no product or companion residual", () => {
  const source = [
    "https://github.com/omesser/ai-buddy/issues/133",
    "AI Buddy reads AI_BUDDY_DIRECTOR and calls ai-buddy-core.",
    "The buddy stands on Buddy Bot.",
    "not an ai-buddy checkout",
  ].join("\n");
  const next = transform(source);
  const hits = next.split("\n").flatMap((line, index) => failingHits("sample.md", line, index + 1));
  assert.deepEqual(hits, []);
  assert.equal(transform(next), next);
});

test("a forge URL is not also counted as the companion word", () => {
  const hits = classifyLine("https://github.com/omesser/ai-buddy/issues/133");
  assert.ok(hits.length > 0);
  assert.ok(hits.every((hit) => hit.role === "forge-slug"));
});

test("the project board sentence does not keep the old title", () => {
  assert.equal(
    transform("Todo on the AI Buddy project board, highest priority first."),
    "Todo on the project board, highest priority first.",
  );
  assert.equal(
    transform("Issues sit on the **AI Buddy** project board (`omesser/ai-buddy`, project number `2`)."),
    "Issues sit on the project board (`omesser/ai-buddy`, project number `2`).",
  );
});
