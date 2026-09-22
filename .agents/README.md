# .agents

Agent assets for this repository. `skills/` is the one skills directory, and
`.claude/skills` and `.cursor/skills` are symlinks to it, so there is a single
copy of the bytes. Both editors glob `<dir>/skills/*/SKILL.md`, and the symlink
sits in the literal prefix of that pattern rather than in what the wildcard
matches, so discovery is plain path traversal into a real directory.

On a Windows clone without `core.symlinks`, git checks each link out as a text
file holding its target path. Both editors then find no skills and an agent
reads `skills/` directly. Only the slash commands are lost.

## `skills/`

One flat directory, three owners. `verify-ai-buddy/` is this repository's own,
hand-written; ADR-0027 covers it. The rest are vendored from
[pstack](https://github.com/cursor/plugins/tree/main/pstack) by Lauren Tan and
from [mattpocock/skills](https://github.com/mattpocock/skills)
(`skills/engineering` only), both MIT. Vendored directories are generated, so an
edit to one is lost on the next sync.

## `pstack/` and `mattpocock/`

Provenance only, no skills. Each holds `UPSTREAM.json`, `LICENSE` and a
generated `SKILLS.md` index, one line per skill from its own frontmatter.

`UPSTREAM.json` records the upstream commit, the licence, the names that sync
owns, and an `excluded` block naming what it deliberately does not vendor and
why. Read it there rather than in prose; it is the record that cannot drift.

Two rules make one directory safe for three owners. **Deletion is gated on the
sync's own name list**, so no sync can remove another's skill or a repo-owned
one. And each sync vendors whatever upstream ships, which is why `EXCLUDED`
exists in both scripts: the lock file's list is a record of the last sync, not a
whitelist gating the next one, so dropping a name from it would not keep the
skill out.

## `agents/`

pstack's agent definitions, vendored the same way. `.claude/agents` is a symlink
to it.

## Changing things

`scripts/sync-pstack.sh --fetch` and `scripts/sync-mattpocock.sh --fetch`
regenerate everything; both run weekly from `.github/workflows/`, open a pull
request only when upstream moved, and never merge.
`tests/skills-layout.test.js` fails when a symlink breaks.
`tests/skills-ownership.test.js` fails if two syncs claim one name, if either
claims a repo-owned skill, or if a skill on disk is claimed by nobody.

Nothing vendored here is on by default. `docs/agents/vendored-skills.md` is the
rule an agent follows when applying one.
