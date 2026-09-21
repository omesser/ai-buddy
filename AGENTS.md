# ai-buddy

## Agent skills

### Issue tracker

Issues live in GitHub Issues on `omesser/ai-buddy`, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles, each label string equal to its name. See `docs/agents/triage-labels.md`.

### Picking work

`Todo` on the AI Buddy project board, highest priority first, matching the OS
you can build. See `docs/agents/picking-work.md`.

### pstack

The pstack skill library is vendored at `.agents/skills/`. Each skill is
a directory holding a `SKILL.md`, and `.agents/pstack/SKILLS.md` indexes all 47
with one line each. None of them is on by default. Apply one when the user names
it, by reading that skill's `SKILL.md` and following it. `poteto-mode` is sticky
once entered — it stays on across turns — so never enter it unprompted. When you
do apply a skill, `docs/agents/pstack.md` records where pstack's opinions
collide with this repository's rules, and this repository's rules win.

### Domain docs

Single-context. Vocabulary is `CONTEXT.md`. v1 scope and requirements are
`docs/SPEC.md`. Decisions live in `docs/adr/` at the repo root. See
`docs/agents/domain.md`.

## Comments

Comments say why, not what, and earn their length. See `docs/agents/comments.md`.

## Writing

Squash merges make the pull request title the only line that reaches `git log`.
An agent signs every comment it writes on GitHub. See `docs/agents/writing.md`.
