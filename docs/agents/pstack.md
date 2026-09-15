# pstack

Reference for when the user asks for a pstack skill. It is not a workflow this
repository runs by default, and nothing here tells an agent to reach for pstack
on its own. `AGENTS.md` carries the availability notice;
`.agents/pstack/SKILLS.md` is the index.

[pstack](https://github.com/cursor/plugins/tree/main/pstack) is a plugin of
agent skills by Lauren Tan: 47 skills and 2 agent definitions covering planning,
review, parallel fan-out, prose discipline, and a set of named engineering
principles. It is MIT licensed. This repository vendors it so that every agent
working here can reach it, not only the ones that install Cursor plugins.

## This repository wins

**Where pstack and this repository disagree, this repository's rule is the one
to follow.** pstack is a general-purpose plugin written for its author's
projects. It ships opinions on pull request shape, commit style, review, prose
voice and autonomy, and this repository already had its own. The table below
names the collisions found when pstack 0.15.2 was vendored. It is not
exhaustive, and the precedence rule holds for a collision the table misses.

| Subject | pstack says | This repository says | Follow |
|---|---|---|---|
| Merging | `skills/poteto-mode/playbooks/shipping.md` step 5 squash-merges a verified pull request, or arms `gh pr merge --auto` | An architect review gate stands on every pull request, and the owner merges | This repository. An agent never merges and never arms auto-merge. |
| Stacked branches | The same playbook lands a stack of pull requests bottom-up, rebasing each onto the one below | Branches come off `main` and merge one at a time | This repository. There is no stack to walk. |
| Pull request title | `skills/poteto-mode/playbooks/opening-a-pr.md` writes Conventional Commits with a lowercase subject: `fix(pstack): retarget opening-a-pr babysit trigger` | `docs/agents/writing.md` writes the same form with the subject capitalized: `feat(engine): Ride a resized Perch under the same gate` | This repository. Capitalize the subject, no full stop. |
| Commit bodies | The same playbook writes every commit body through `/technical-writing` | `main` takes squash merges, so a branch commit body never reaches `git log`. Keep it short or leave it out | This repository. Reasoning that has to outlive review goes in the pull request description or a code comment. |
| Code comments | The `no-comments` skill and the `comment-sicko` agent delete comments and keep one only for a *why* the code cannot show | `docs/agents/comments.md` keeps a reason, a constraint, a rejected alternative, or a bound that looks arbitrary and is not, and `ponytail:` markers record deliberate shortcuts | This repository. Do not run `/no-comments` over this codebase. |
| The `comment-sicko` subagent | `.claude/agents` is symlinked, so `comment-sicko` is registered and invocable by name. Its whole job is deleting comments | The same `docs/agents/comments.md` rule | This repository. It is reachable, not sanctioned. Registering an agent does not run it, and nothing here should. |
| Prose voice | The `unslop` skill forbids em dashes outright; `technical-writing` layers Diátaxis, Google style, STE and Global English | `docs/agents/writing.md` hands voice to the `developer-voice` skill, and the prose here uses em dashes throughout | This repository. Read pstack's writing skills for ideas, not as a licence to rewrite existing prose. |
| Signing | pstack's pull request guidance carries no signature | `docs/agents/writing.md` ends every issue comment, pull request description, review and inline reply an agent writes with its own sign-off line | This repository. The sign-off is additive; pstack does not remove it. |
| Picking work | `poteto-mode` autonomy and `principle-never-block-on-the-human` push an agent to choose and proceed | `docs/agents/picking-work.md` picks only `👷 Todo` on project board 2, reads four signals in order, and self-assigns first | This repository. Autonomy applies inside an issue you were given, not to choosing one. |
| Where writing lands | pstack playbooks emit plans, sketches and design notes as they go | `docs/agents/docs.md` decides what is an ADR, what is `docs/DEVELOPMENT.md`, and what is scratch | This repository. Session scratch is not a document; `docs/superpowers/` is gitignored for that reason. |

## Where it lives

`.agents/skills/` is the one skills directory for this repository. The 47
vendored pstack skills sit in it alongside `verify-ai-buddy`, which this
repository wrote and owns. `.agents/agents/` holds pstack's two agent
definitions.

`.agents/pstack/` holds provenance and no skills. `UPSTREAM.json` records the
upstream commit, the plugin version, and the list of skill names the sync owns.
The sync vendors whatever upstream ships and rewrites that list from what it
found, so a skill added upstream arrives without anyone editing the list. What
the list gates is deletion: only a name already on it can be removed, which is
what keeps `verify-ai-buddy` safe from an upstream that has never heard of it.
`.agents/agents/` has no such list — it is wholly upstream's, and the sync
replaces it outright. `LICENSE` carries the MIT text.

The upstream guide (`pstack/docs/`), logo and automations are not vendored.
Read them at the source.

## Invoking a skill

Every skill is a directory under `.agents/skills/` holding a `SKILL.md`,
sometimes with `references/`, `playbooks/` or `scripts/` beside it. Read the
`SKILL.md` and follow it. That is the whole mechanism, and it works in any agent
that can read a file.

Claude Code and Cursor also discover them natively. `.claude/skills` and
`.cursor/skills` are symlinks to `.agents/skills`, and `.claude/agents` is a
symlink to `.agents/agents`, so `/unslop` and the rest resolve without a path.
Both editors glob `<dir>/skills/*/SKILL.md`, and the symlink sits in the literal
prefix of that pattern rather than in what the wildcard matches, so discovery is
plain path traversal into a real directory. There is one copy of the bytes.
`scripts/sync-pstack.sh` creates the links and `tests/skills-layout.test.js`
fails when one goes missing, dangling, or points somewhere unexpected.

On Windows this degrades. Git checks a symlink out as a plain text file holding
its target path unless `core.symlinks` is on, which needs Developer Mode or an
elevated shell. On such a clone `.claude/skills` is a one-line text file, both
editors find no skills, and an agent reads `.agents/skills/` directly, which is
the path `AGENTS.md` names. Nothing is lost except the slash commands.

`.agents/pstack/SKILLS.md` lists all 47 with the one-line "use it when" each
skill writes in its own frontmatter. It is generated by the same script, so it
cannot drift from what is vendored.

Two cautions. Several skills declare themselves always-on — `unslop` says "Must
always apply" — and that declaration does not carry here; none of them is on
until the user names it. `poteto-mode` is a mode rather than a one-shot: it
stays on across turns once entered, and it routes work into the playbooks the
collision table above overrules. Enter it only when asked for it by name.

## Branch work goes in a worktree

pstack's `opening-a-pr` playbook already says to work from a git worktree off
`main`. This repository enforces it harder: never run git, cargo, or a build in
the primary checkout. Branch work and test runs happen in a worktree.
`.gitignore` excludes `.worktrees/` and everything in `.claude/` except the two
symlinks, for that reason.

## Keeping it current

`.github/workflows/pstack-sync.yml` runs weekly and on demand. It re-runs
`scripts/sync-pstack.sh --fetch`, which re-fetches `pstack/skills` and
`pstack/agents` from `cursor/plugins@main`, rewrites `UPSTREAM.json`, and
regenerates `SKILLS.md`. The workflow opens a pull request when the tree
actually changed, and never merges. It applies no label: every string in
`docs/agents/triage-labels.md` is an issue triage role, and a pull request
already arrives needing review. The pull request carries the upstream SHA range
and a diff summary so a reviewer does not have to read 124 files.

A pull request opened with `GITHUB_TOKEN` does not trigger `on: pull_request`,
so `PR Tests` never fires for a sync pull request. The workflow runs
`tests/skills-layout.test.js` itself before opening one, then dispatches the
full suite at the sync branch — `workflow_dispatch` is one of the two events
GitHub exempts from that suppression, so it fires where `pull_request` does not.
A dispatched run is standalone, though. The pull request still carries no `PR
Tests` check, however green the suite came out, so read the run itself; the
description links it. Turning that into a check mark would take a GitHub App
token, which this repository deliberately does not carry.

To sync by hand, run `scripts/sync-pstack.sh --fetch`.
