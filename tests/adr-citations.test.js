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

// Code citations of a superseded ADR still in the tree. Markdown, `docs/` and
// `.agents/` are exempt. Fix a site, lower the count here. An entry at zero
// is deleted. A count that no longer matches the tree fails this test.
const KNOWN_STALE = {
  "src-tauri/src/acp_wire.rs": 1,
  "src-tauri/src/harness.rs": 3,
  "src-tauri/src/main.rs": 7,
  "src-tauri/src/model.rs": 2,
  "src-tauri/src/settings.rs": 7,
  "src-tauri/src/settings/form.rs": 4,
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
