# Writing

Voice belongs to the
[`developer-voice`](https://github.com/omesser/ai-goodies/tree/main/skills/developer-voice)
skill. Read it there if you do not have it loaded. This file holds the one thing
no skill can know: where writing lands in this repository.

## The pull request title is the line that survives

`main` takes squash merges only. The commit subject is the pull request title
and the body is empty, so nothing else reaches `git log`.

- Write the title as the one-sentence summary a reader should find a year from
  now.
- Branch commit bodies serve the reviewer and the merge discards them. Keep them
  short, or leave them out.
- Reasoning that has to outlive the review goes in a code comment or the pull
  request description. `docs/agents/comments.md` covers the first.

That last point overrides `developer-voice`, which puts the reasoning in the
commit body. Here the body does not survive.

## Pre-commit hooks gate every commit

Run `pre-commit run --files <touched>` or `pre-commit run --all-files` before
every commit and push. Do not use `--no-verify` to skip hooks. The suite
catches trailing whitespace, codespell findings, formatting drift, and clippy
warnings — all of which would fail in CI. Hooks that autofix (trailing
whitespace, formatters) rewrite files in place; stage the fixes and commit
again.

## The title carries its type

Titles follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```
<type>[optional scope]: <description>
```

The type is what the reader scanning `git log` wants first — whether a line is a
new capability, a repair, or housekeeping they can skip. Nothing here is
released, so no version is derived from it; the value is legibility.

| Type | For |
|---|---|
| `feat` | A new capability |
| `fix` | A repair to one that misbehaves |
| `docs` | Documentation only, including `CONTEXT.md`, `DESIGN.md` and `docs/SPEC.md` (see `docs/agents/docs.md` for where writing belongs) |
| `test` | Tests and the verification harness, with no change to what ships |
| `refactor` | A change that keeps behavior and alters structure |
| `perf` | A change made for speed or footprint |
| `ci` | Workflows, hooks, and the checks that gate a merge |
| `build` | Dependencies, the Cargo workspace, packaging |
| `chore` | Everything else that touches no behavior |
| `revert` | Undoing a merged change |

Two rules the type table cannot settle:

- **Classify by what the change is for, not by which files it touches.** A fix
  that ships with the documentation correcting it is a `fix`. Tests written for
  behavior landing in the same pull request are part of that `feat`; `test` is
  for a pull request whose product *is* the test.
- **A scope is optional and only earns its place when it narrows something.**
  `engine`, `shell`, `character`, `harness`. Skip it when the description
  already says where the change lives.

The description after the colon keeps the imperative sentence this repository
has always written: `feat(engine): Ride a resized Perch under the same gate`.
Capitalized, no full stop. Conventional Commits does not rule on case, and
matching the existing history matters more than matching other projects.

Mark a breaking change with `!` before the colon — `feat(engine)!: ...` — and
say what breaks in the description.

## The description answers three questions

`.github/pull_request_template.md` asks them: **Why**, **What changed**, and
**How to verify**. Why comes first because a reviewer who does not know the
problem cannot judge the solution — a description that opens with what it added
makes them reconstruct the problem from the diff.

A pull request small enough that all three answers are one sentence each should
give one sentence each. The template is a floor, not a quota.

## A visual change shows itself

When a pull request changes what the product looks like, the description
carries the artifact: the new asset, and the old one beside it when the change
*is* the difference between them. A reviewer who would have to build the branch
to see a corner radius does not build the branch — they approve on the diff,
which is the one thing that cannot show them a corner radius. Motion is no
exception; a contact sheet of frames says more than a sentence about timing.

Upload the image with `gh pr create --attach` or `gh pr comment --attach`, in
`<file>#<alt text>` form. GitHub rewrites a body reference such as
`![alt](./before.png)` to point at the uploaded asset, which outlives the branch
and the squash merge. Until gh 2.101.0 this page said to commit the image
instead, because attachment upload was a browser affordance no agent could
reach. That is no longer true, and a still committed only to be looked at once
leaves a file in the tree that nothing references after the merge.

Check an image in only when it documents an asset that lives here.
`branding/app-icon-preview.png` is the worked example, and it earns its place
next to the icon it previews, not by being in a pull request description.

## An agent signs what it writes on GitHub

End every issue comment, pull request description, review and inline reply an
agent writes with one line of its own. There are two kinds of signer. The first
words of the line say which, so a Grok Build session and a Grok bot named Coder
cannot be mistaken for each other.

**A Harness agent** names the Harness that launched the session, then `agent`:

```
_— Grok Build agent, on [@omesser](https://github.com/omesser)'s behalf._
```

The Harness is the product, written as the README Harness Support table already
names it for humans: `Cursor`, `Claude Code`, `Grok Build`, `Codex`,
`OpenCode`, `Hermes`, `Pi`. An unnamed attach uses that command (`copilot`,
not a made-up brand). Do not add a parenthetical after `agent`. `Coder` and
`Architect` are Grok bot instance names. `_— Cursor agent (Coder)_` and
`_— Grok Build agent (Coder)_` both read as that bot.

**A Grok bot** (a named bot in Grok, not a Harness attach) names the bot:

```
_— Grok bot (Coder), on [@omesser](https://github.com/omesser)'s behalf._
```

`<Name>` is the bot's configured name. Only this line may use those names.

This applies to every issue, PR body, review, comment, and gist on any GitHub
repository when writing through Oded's account.

When an agent writes through an owner's account, the author name never says
who wrote a comment. A reader following a back-and-forth needs that: which line
to argue with, and which is a decision already taken. `Co-authored-by` records
authorship on commits only, and GitHub has no author field an agent can set.

Sign once per body, at the end. What the owner writes stays unsigned, so the
missing line carries meaning too. If an agent revises a body the owner wrote,
leave that text unsigned and say what changed in a comment of its own.
