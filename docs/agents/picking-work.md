# Picking work

Issues for this repo sit on the **AI Buddy** project board (`omesser/ai-buddy`,
project number `2`). Read four signals before you claim one: board status,
milestone, priority, and operating system. Read them in that order. A later
signal never rescues an issue an earlier one ruled out.

## Board status

The board's `Status` field has five values.

| Status | What it means for you |
|---|---|
| `👷 Todo` | Scoped and unclaimed. This is the only status you pick from. |
| `🏗 In Progress` | Someone is already on it. Never take it. |
| `🙋 Human Triage` | A human still has to decide what this is. Leave it. |
| `🍨 Ice Box` | Deferred on purpose. Do not revive it without asking. |
| `🏁 Done` | Finished. Nothing to do. |

Status lives on the board, not on the issue, so `gh issue list` cannot see it.
Read it with `gh project item-list`:

```sh
gh project item-list 2 --owner omesser --limit 300 --format json \
  --jq '.items[] | select(.status == "👷 Todo") |
        "\(.content.number)  \(.labels // [] | join(","))  \(.content.title)"'
```

`--format json` carries the full body of every issue, so filter with `--jq` and
never print the raw output.

An item whose `linked pull requests` field holds an open PR is being worked on
even when its status still reads `👷 Todo`. Treat it as taken.

## Milestone

`V1` is the current milestone. `V2` is the next one, and an issue parked there
is not this week's work.

`.github/workflows/default-milestone.yml` runs on `issues: opened` and sets `V1`
on any issue that arrives without a milestone. It never overrules a milestone
the author picked, and it does not touch pull requests. So an open issue with no
milestone is rare: it means someone cleared the milestone by hand after the
issue was filed. Treat that issue as unscoped and leave it.

## Priority

Three labels rank the work, `p0` over `p1` over `p2`.

| Label | Meaning |
|---|---|
| `p0` | Drop everything: broken for users, or blocking |
| `p1` | Next up: the default for work that should happen |
| `p2` | Eventually: real, but nothing waits on it |

Take the highest priority available rather than the most interesting issue. When
two issues share a priority, the operating system decides.

## Operating system

The labels `macos`, `linux`, and `windows` mark which platform an issue lands
on. Prefer one that matches the machine you are running on.

The reason is verification. `src-tauri/src/platform/` holds three native
renderers — `macos/`, `windows/`, and `x11/` — each behind `cfg(target_os)`, so
only the host's renderer compiles on any given machine. Take an issue for
another platform and you ship a fix you cannot build, run, or see.

List the issues for your platform with:

```sh
gh issue list --state open --label macos --json number,title,labels
```

On a pull request, the `windows` label does a second job: it forces the Windows
compile job in `.github/workflows/tests.yml` to run. Applying it to a PR is a CI
decision, not a claim about which platform the work belongs to.

## Triage labels

Two labels from `docs/agents/triage-labels.md` overrule everything above.

- `ready-for-agent` — fully specified. Take it.
- `ready-for-human` — requires human implementation. Do not take it, whatever
  its status or priority says.

## Claiming it

Assign yourself before you write any code, so the next agent reading the board
sees the issue is gone:

```sh
gh issue edit <number> --add-assignee @me
```

Then move the board item to `🏗 In Progress`.
