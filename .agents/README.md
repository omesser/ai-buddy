# .agents

Agent assets for this repository. `skills/` is the one skills directory, and
`.claude/skills` and `.cursor/skills` are symlinks to it, so there is a single
copy of the bytes.

## `skills/`

Two kinds of directory, side by side.

`verify-ai-buddy/` is this repository's own, hand-written, and moved here from
`.cursor/skills/` so that Claude Code can see it too. ADR-0027 covers it.

Everything else is vendored from
[pstack](https://github.com/cursor/plugins/tree/main/pstack) by Lauren Tan, MIT
licensed. Those directories are generated. An edit to one is lost on the next
sync.

## `agents/`

pstack's two agent definitions, vendored the same way. `.claude/agents` is a
symlink to it.

## `pstack/`

Provenance only, no skills. `UPSTREAM.json` records the upstream commit, the
plugin version, and the list of skill names the sync owns. That list is what
keeps `verify-ai-buddy` safe: the sync adds, updates and deletes only names it
finds there. `LICENSE` is the MIT text carried with the copy. `SKILLS.md` is the
generated index of the vendored skills, one line each.

## Changing things

`scripts/sync-pstack.sh` regenerates all of it, and
`.github/workflows/pstack-sync.yml` runs it weekly. `tests/skills-layout.test.js`
fails when a symlink is missing, dangling, or pointing somewhere unexpected.

Nothing vendored here is on by default. An agent applies a skill when the user
names it, and not otherwise. To change how pstack applies in this repository,
write in `docs/agents/pstack.md`, which also records which rule wins where
pstack and this repository disagree. This repository always wins.
