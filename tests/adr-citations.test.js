// #924: a comment citing a superseded ADR sends the reader to a document whose
// first line tells them to go somewhere else, and they follow it because a
// citation reads as authority. Markdown is exempt, since narrating a
// supersession is what prose is for, and `docs/design` pages are Dated
// snapshots (ADR-0011) that must keep the link they shipped with.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const repo = new URL("..", import.meta.url);

// The forty #924 left standing. Each one wants a rule, a state or a tier that
// the superseding ADR dropped as "implementation detail that belongs
// elsewhere" and that nothing live picked up, so repointing the number would
// only move the reader to a document that does not carry the claim either.
// #962 is the argument. Fix a site, lower the count here; an entry that has
// gone stale is the failure telling you the work landed.
const KNOWN_STALE = {
  "Cargo.toml": 1,
  "crates/core/src/character.rs": 1,
  "crates/core/src/sensing.rs": 1,
  "src-tauri/Cargo.toml": 1,
  "src-tauri/src/acp_wire.rs": 2,
  "src-tauri/src/harness.rs": 5,
  "src-tauri/src/main.rs": 8,
  "src-tauri/src/model.rs": 2,
  "src-tauri/src/platform/macos/sensing.rs": 1,
  "src-tauri/src/settings.rs": 8,
  "src-tauri/src/settings/form.rs": 5,
  "src/bubble.js": 1,
  "src/chat-ui.css": 1,
  "src/main.css": 2,
  "tests/bubble.test.js": 1,
};

const tracked = (...args) =>
  execFileSync("git", ["ls-files", "-z", ...args], {
    cwd: repo,
    encoding: "utf8",
  })
    .split("\0")
    .filter(Boolean);

function supersededAdrs() {
  const numbers = new Set();
  for (const path of tracked("docs/adr")) {
    const status = readFileSync(new URL(path, repo), "utf8").match(
      /^\*\*Status:\*\*\s*(.*)$/m,
    );
    if (status && /^superseded/i.test(status[1])) {
      numbers.add(path.match(/\d{4}/)[0]);
    }
  }
  return numbers;
}

function citedStale(superseded) {
  const counts = {};
  for (const path of tracked()) {
    const exempt =
      path.endsWith(".md") ||
      path.startsWith("docs/") ||
      path.startsWith(".agents/");
    if (exempt) continue;
    let text;
    try {
      text = readFileSync(new URL(path, repo), "utf8");
    } catch {
      continue; // unreadable as text, so it holds no citation
    }
    const hits = text.match(/ADR[- ]?\d{4}|adr\/\d{4}-/g) ?? [];
    const stale = hits.filter((hit) =>
      superseded.has(hit.match(/\d{4}/)[0]),
    ).length;
    if (stale) counts[path] = stale;
  }
  return counts;
}

test("no code cites a superseded ADR beyond the #962 backlog", () => {
  const superseded = supersededAdrs();
  assert.ok(superseded.size > 0, "the ADR status lines still parse");
  assert.deepEqual(
    citedStale(superseded),
    KNOWN_STALE,
    "a count that rose is a new stale citation; one that fell means KNOWN_STALE is behind",
  );
});
