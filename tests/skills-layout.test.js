// Run with `node --test tests/*.test.js`.
//
// `.agents/skills/` holds every skill this repository offers an agent, both the
// vendored pstack ones and its own. Claude Code and Cursor each glob
// `<dir>/skills/*/SKILL.md`, so `.claude/skills` and `.cursor/skills` are
// symlinks to it rather than copies. The symlink sits in the literal prefix of
// that pattern, before the wildcard, so discovery is plain path traversal into
// a real directory.
//
// One copy of the bytes means there is no mirror to drift. What can still
// break is the link itself: deleted by a tool that rewrites its own config,
// replaced with a directory, or pointed somewhere that no longer exists. Each
// of those makes every skill silently invisible, which is exactly the failure
// nobody notices. This test is what notices.

import assert from "node:assert/strict";
import { lstatSync, readFileSync, readdirSync, readlinkSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const root = new URL("../", import.meta.url);
const path = (p) => fileURLToPath(new URL(p, root));

// Where the bytes live, and the two links that must reach them.
const REAL = ".agents/skills";
const LINKS = [
  [".claude/skills", "../.agents/skills"],
  [".cursor/skills", "../.agents/skills"],
];

// A skill this repository owns rather than vendors. It proves the directory is
// shared, so a sync that blew it away would fail here rather than in six
// months when someone next runs the verifier.
const OURS = "verify-ai-buddy";

const skillsIn = (dir) =>
  readdirSync(path(dir), { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort();

test("the real skills directory holds a non-zero number of skills", () => {
  const names = skillsIn(REAL);
  assert.ok(
    names.length > 0,
    `${REAL} is empty; every assertion below would pass over nothing`,
  );
  assert.ok(
    names.includes(OURS),
    `${REAL} has lost ${OURS}, which this repository owns. The pstack sync must never remove it.`,
  );
});

for (const [link, target] of LINKS) {
  test(`${link} is a symlink to ${target}`, () => {
    let stat;
    try {
      stat = lstatSync(path(link));
    } catch (err) {
      assert.fail(
        `${link} does not exist (${err.code}). Skill discovery is dead. Run scripts/sync-pstack.sh.`,
      );
    }
    assert.ok(
      stat.isSymbolicLink(),
      `${link} is not a symlink. One copy of the bytes is the point; run scripts/sync-pstack.sh.`,
    );
    assert.equal(
      readlinkSync(path(link)),
      target,
      `${link} points somewhere unexpected. Run scripts/sync-pstack.sh.`,
    );
  });

  test(`${link} resolves to the skills directory`, () => {
    let stat;
    try {
      // statSync follows the link. A dangling one throws here, which is the
      // case a readlink check alone would miss.
      stat = statSync(path(link));
    } catch (err) {
      assert.fail(
        `${link} is dangling (${err.code}); it points at nothing. Run scripts/sync-pstack.sh.`,
      );
    }
    assert.ok(stat.isDirectory(), `${link} does not resolve to a directory`);

    const through = skillsIn(link);
    assert.deepEqual(
      through,
      skillsIn(REAL),
      `${link} does not see the same skills as ${REAL}`,
    );
    assert.ok(through.includes(OURS), `${link} cannot see ${OURS}`);
  });

  test(`${link} serves a readable SKILL.md for every skill`, () => {
    // What the loader's `<dir>/skills/*/SKILL.md` glob actually needs.
    for (const name of skillsIn(link)) {
      const file = `${link}/${name}/SKILL.md`;
      let body;
      try {
        body = readFileSync(path(file), "utf8");
      } catch (err) {
        assert.fail(`${file} is not readable through the symlink (${err.code})`);
      }
      assert.match(body, /^---\r?\n/, `${file} has no frontmatter`);
    }
  });
}
