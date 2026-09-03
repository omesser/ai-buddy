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
| `docs` | Documentation only, including `CONTEXT.md`, `DESIGN.md` and `docs/SPEC.md` |
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

## An agent signs what it writes on GitHub

End every issue comment, pull request description, review and inline reply an
agent writes with one line of its own:

```
_— Cursor agent, on @omesser's behalf._
```

When an agent writes through an owner's account, the author name never says
who wrote a comment. A reader following a back-and-forth needs that: which line
to argue with, and which is a decision already taken. `Co-authored-by` records
authorship on commits only, and GitHub has no author field an agent can set.

Sign once per body, at the end. What the owner writes stays unsigned, so the
missing line carries meaning too. If an agent revises a body the owner wrote,
leave that text unsigned and say what changed in a comment of its own.
