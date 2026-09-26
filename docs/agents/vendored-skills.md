# Vendored skills

Two third-party skill sets are vendored into `.agents/skills/`: pstack, and Matt
Pocock's engineering set. This page is the rule for applying one.
`.agents/README.md` covers where the bytes live and what each sync owns.

## The rule

**Nothing here is on by default.** Apply a skill when the user names it, by
reading its `SKILL.md` and following it. Several declare themselves always-on —
`unslop` says "Must always apply" — and that does not carry here. `poteto-mode`
stays on across turns once entered, so enter it only when asked for by name.

**Where a skill and this repository disagree, this repository wins.** Both sets
were written for their authors' projects. The table is not exhaustive; the rule
holds for a collision it misses.

**A path a vendored skill names is relative to that skill, not to the
repository root.** `poteto-mode`'s babysit playbook says PR status comes from
`scripts/watch-pr/watch-pr`; here that is
`.agents/skills/poteto-mode/scripts/watch-pr/watch-pr`, and it runs — it needs
`bun`, which the launcher's shebang names. A few references instead carry
upstream's own plugin root, as in `pstack/skills/poteto-mode/scripts/check-plan.mjs`;
drop the `pstack/skills/` prefix and read the rest under `.agents/skills/`.
Neither shape resolves from the repository root, and an agent that gets "no such
file" there quietly invents a workaround rather than reporting the miss.
`tests/skills-script-refs.test.js` resolves every runnable path the skills name
and fails on one that is genuinely absent.

**A personal copy beats the vendored one.** Claude Code resolves enterprise over
personal over project, so a skill in someone's `~/.claude/skills/` shadows the
vendored copy and the vendored copy is what everyone else gets. Cursor
documents no order for a name clash — do not assume it matches.

## Where they collide

| Subject | Set | The skill says | Follow |
|---|---|---|---|
| Merging | pstack | `poteto-mode`'s shipping playbook squash-merges a verified pull request, or arms `gh pr merge --auto` | An architect gate stands on every pull request and the owner merges. An agent never merges and never arms auto-merge. |
| Branch shape | Both | pstack lands a stack of pull requests bottom-up; `prototype` commits a throwaway to its own branch | Branches come off `main` and merge one at a time, in a worktree. A throwaway branch is fine; a stack is not. |
| Pull request title | pstack | Conventional Commits with a lowercase subject | `docs/agents/writing.md` capitalizes the subject, no full stop. |
| Commit bodies | pstack | Every commit body written through `/technical-writing` | Squash merges discard the body. Reasoning that outlives review goes in the description or a code comment. |
| Code comments | pstack | `no-comments` and the `comment-sicko` agent delete comments, keeping only a *why* the code cannot show | `docs/agents/comments.md` sets a 3-line default and names the exceptions that beat it. Do not run `/no-comments` here. `comment-sicko` is reachable through the symlink, not sanctioned. |
| Prose voice | pstack | `unslop` forbids em dashes outright | The prose here uses them throughout. Read the writing skills for ideas, not as a licence to rewrite existing prose. |
| Signing | pstack | Its pull request guidance carries no signature | `docs/agents/writing.md` signs every GitHub body. The sign-off is additive. |
| Picking work | pstack | `poteto-mode` autonomy and `principle-never-block-on-the-human` push an agent to choose and proceed | `docs/agents/picking-work.md` decides. Autonomy applies inside an issue you were given, not to choosing one. |
| Filing an issue | Matt Pocock | `to-spec` and `to-tickets` publish to the tracker with a `ready-for-agent` label | A label alone leaves an issue unpickable: it also needs a board status, the `V1` milestone and a priority label. |
| Where writing lands | Both | pstack emits plans and design notes as it goes; `research` saves findings "where the repo already keeps such notes" | `docs/agents/docs.md` decides. A `docs/research/` document must name its commit anchor — `tests/research-anchors.test.js` turns CI red without one. |
| Recording a decision | Matt Pocock | `domain-modeling` creates `docs/adr/` lazily, whenever a decision seems worth writing down | An ADR passes three gates at once: hard to reverse, surprising without context, a real trade-off. |
| Test-driven development | Both | Both sets ship a `tdd`, and `implement` reaches for it by name | Matt Pocock's. pstack's is excluded so that reference resolves to the loop it means. |
| Generated shell | Matt Pocock | `wizard` generates an interactive bash wizard | `shfmt` and `shellcheck` gate shell in this tree. Keep it out of the tree, or make it pass. |
| Repo setup | Matt Pocock | `setup-matt-pocock-skills` step 4 edits `CLAUDE.md` when it exists | Not vendored, because it has already run here. If you fetch it by hand, point it at `AGENTS.md`: `CLAUDE.md` here only points at `AGENTS.md`, so step 4 would write a second skills block into the wrong file. |
