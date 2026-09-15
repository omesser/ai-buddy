# CPU wakeups and idle C-state impact on macOS

Baseline for #431, under #423's plan. #423 names the suspect: an unconditional
~60 Hz frame-loop tick that never backs off, even when the sprite is still and
the desktop is idle (#183). PR #718 is open right now, shipping idle
back-off for macOS and Windows against that suspect (#183 Stage 2b); this
document gives the baseline that makes its claim checkable, measured on both
`main` and #718's head so the delta is a number, not an assertion.

## Headline, read this before the tables

**#431's own hypothesis is rejected as stated, and the reason is not fully
identified.** #431 predicts ~60 wakeups/sec from the 16 ms tick. Measured:
this process sits at **~300–450 wakeups/sec**, on `main` and on #718's head,
in every scenario, across nine separate captures. The frame loop's 16
ms-vs-1 s tick difference cannot explain a gap that size. Something else in
this process — a WKWebView event pump, the async runtime, log rotation, the
tray icon are candidates, in descending order of how much of this process's
own thread activity they plausibly own — accounts for the bulk of it. **This
is a guess, not a finding**: nothing in this task isolated which one, or
whether it is several of them at once. A follow-up that wants the real
number needs `sample` or a per-thread breakdown, not
`powermetrics --samplers tasks`, which only totals wakeups at the process
level.

**#718's idle-perched effect is directionally real and not resolved at this
sample size.** An earlier draft of this document reported a single 60-second
capture per branch and stated the gap between them — 3.59 vs. 0.8–1.3
package-idle wakeups/sec — as a **65–75% reduction**, in bold, as a measured
finding. That was arithmetic on one observation per arm dressed as a
confidence interval, and it does not survive a repeat: three idle-perched
captures per branch, interleaved A/B/A/B/A/B so a drift in machine load lands
on both arms rather than one, put `main`'s three runs at **2.92, 4.22,
5.09** package-idle wakeups/sec (median 4.22) and #718's three runs at
**3.24, 3.52, 4.32** (median 3.52). The ranges overlap almost entirely — #718's
whole range sits inside `main`'s. The median gap (4.22 vs. 3.52, about 17%)
points the same direction the original 65–75% figure did, but at n=3 per arm
on this machine, that direction is not distinguishable from the run-to-run
noise each arm shows on its own (`main` alone spans 2.92 to 5.09, a wider
range than the gap between the two arms' medians). **The honest statement of
this baseline is: #718 likely helps, and this data cannot yet say by how
much.** Full tables below.

**Read every number below against a shared, noisy machine, not a clean-room
rig.** This is a dev laptop with other agents building and running their own
work on it throughout every capture in this document, including their own
`ai-buddy` instances. That is not a hypothetical caveat: it is the direct
cause of the overlap above, and separately, re-measuring the chat-open
scenario about fifteen minutes after the first attempt moved that scenario's
package-idle wakeups by ~60% on **both** branches, with no code change. A
number in this document is a sample from a moving system, not a constant.
Rerunning this on a quiet machine should be expected to produce different
figures, and that expectation is part of the result, not a footnote to it.

## The machine and the build

| | |
|---|---|
| Machine | MacBook Pro `Mac15,7`, Apple M3 Pro, 36 GB |
| OS | macOS 26.6.2 (25G83) |
| Build | `target/release/ai-buddy`, **release**, per branch, ad-hoc signed at its worktree path |
| Director | `AI_BUDDY_DIRECTOR=0` — `StaticDirector`, offline, no HTTP leaves the process |
| Character | BMO (the default when none is set) |
| Date | 2026-09-15 |
| Control arm, written `main` below | `7a58e02f` — this issue's base, **before** #718 |
| Treatment arm, written `#718` below | `9864d789` (`cursor/macos-idle-backoff-183-3b49`) |

**#718 has since merged** (`8ba9481d`). So the arm labelled `#718` throughout this
document is what `main` does today, and the arm labelled `main` is the pre-back-off
state that no longer exists on the branch. The A/B below is unaffected: it compares two
fixed commits and both are still in history. What changed is which arm a future reader
should treat as the baseline to beat. That is the `#718` column, not the `main` one.

Both binaries were built with `cargo build -p ai-buddy --release` from a
dedicated worktree per branch (`.worktrees/431-macos-wakeups` and
`.worktrees/718-idle-backoff`), never in the primary checkout.

**This is a shared dev machine, not a clean-room rig.** Other agents were
building and running their own work on it throughout this capture, including
their own `ai-buddy` instances (a concurrent, unrelated `ai-buddy` process
under a different PID showed up in a real capture during this task — the
tooling below filters by exact PID for that reason, see Gotchas). Per-process
wakeup counts are scoped by PID, which rules out double-counting a different
process, but does not rule out this process itself running busier or
quieter depending on what else the machine is doing at that moment — see
Headline above for two measured examples of exactly that, one of which
(idle-perched) is why this document reports medians and ranges rather than
single numbers. The `cpu_power` sampler's idle-residency and package-power
numbers are **system-wide**, not per-process, so they carry the same
confound a second time, more directly.

## Tools

- `scripts/bench-wakeups-macos.sh --binary PATH --scenario idle|chat [--duration SECS] [--out DIR]` —
  launches the release binary with the env a worktree build needs
  (`AI_BUDDY_DIRECTOR_API_KEY` skips the Keychain prompt, #283), waits for the
  overlay to report ready, optionally drives Summon (see below), then runs
  `sudo powermetrics -i 1000 -n DURATION --samplers tasks,cpu_power` for the
  window and writes `powermetrics.txt` + `app.log` + `meta.txt` to `--out`.
- `scripts/parse-powermetrics.py POWERMETRICS_TXT --pid PID [--frame-log APP_LOG]` —
  reduces the capture to wakeups/sec (interrupt and pkg-idle), CPU%, cluster
  idle residency and package power, for the exact PID. With `--frame-log` it
  also buckets seconds by whatever animation the frame trace says was on
  screen at that wall-clock second (idle-family vs. active, and per
  animation), so a single idle-perched capture that happens to catch a
  natural walk answers two scenarios at once.
- `scripts/click-cursor.swift x y [clicks]` — posts one or two real HID
  left-clicks at a point, warping the cursor there first. **Shared with
  #728** (`crates/verify`'s `summon`/`poke` subcommands add the same path;
  one click poster lives in the tree, not two — this document's tooling
  calls #728's script rather than forking it). Needed because
  `osascript ... System Events click at` resolves an Accessibility UI
  element under the point first, and this overlay's borderless
  always-on-top panel does not present one to resolve — it fails with error
  -25208 even after the app's own hit-test has already flipped click-through
  off. `CGEventPost` needs the same Accessibility grant and posts to the HID
  event stream directly, the same path a real click takes. Confirmed against
  the running app's own `verbs: ... [Poke]` / `[Summon]` trace lines, not
  assumed.
- `sudo -n powermetrics` worked non-interactively in this environment (no
  password prompt). If a reviewer's machine prompts, powermetrics needs a
  session where `sudo` is already primed, or these numbers are not
  reproducible unattended — see Gotchas.

Every scenario's Summon (chat-open) uses `click-cursor.swift $CX $CY 2` at
the sprite's on-screen centre, read from the app's own `frame:` trace line,
after waiting for a `Grounded` or `Perched` state (the sprite spawns mid-air
and plays `Falling` first — clicking during that window misses because the
art has moved by the time the click lands, discovered by hitting it
directly). Every chat-scenario run in this document confirmed
`verbs:.*Summon` in the log; a capture that could not confirm it is not in
this document.

## Results: idle-perched, interleaved

Three 45-second captures per branch, run **A, B, A, B, A, B** (`main`, #718,
`main`, #718, `main`, #718) rather than all of one branch then all of the
other, so a drift in shared-machine load lands on both arms in the same
round instead of correlating with one branch. All numbers below are the
idle-family bucket (`idle` + `sit` animation-seconds) from
`scripts/parse-powermetrics.py --frame-log`.

| Run | Branch | n | Wakeups/sec (interrupt) | Wakeups/sec (pkg-idle) | CPU% |
|---|---|---|---|---|---|
| 1 | `main` | 37 | 369.1 | 5.09 | 14.6 |
| 2 | `main` | 37 | 369.0 | 4.22 | 14.6 |
| 3 | `main` | 37 | 387.5 | 2.92 | 22.3 |
| 1 | #718 | 36 | 368.5 | 4.32 | 14.1 |
| 2 | #718 | 35 | 300.4 | 3.52 | 14.9 |
| 3 | #718 | 37 | 384.5 | 3.24 | 21.4 |

Round 3's CPU% jump (22.3% and 21.4%, versus ~14–15% in rounds 1–2) landed on
**both** branches in the same round — direct evidence the interleaving is
doing its job: a load spike shows up as a shared-round effect, not as a
branch effect, which is exactly why it does not need to be thrown out.

**Median and range per arm:**

| | `main` (n=3 runs) | #718 (n=3 runs) |
|---|---|---|
| Wakeups/sec (pkg-idle), median [range] | 4.22 [2.92–5.09] | 3.52 [3.24–4.32] |
| Wakeups/sec (interrupt), median [range] | 369.1 [369.0–387.5] | 368.5 [300.4–384.5] |
| CPU%, median [range] | 14.6 [14.6–22.3] | 14.9 [14.1–21.4] |

**The ranges overlap.** #718's package-idle range (3.24–4.32) sits almost
entirely inside `main`'s (2.92–5.09). The median difference (4.22 vs. 3.52)
is real and points toward #718 helping, but it is smaller than the spread
each branch shows against itself across three runs on this machine, so this
data does not resolve the effect size — only its direction, tentatively.

## Walking

Never triggered synthetically (this harness posts no synthetic walk into the
app); `StaticDirector` picked a `walk`-containing Behavior on its own inside
five of the six idle-perched captures above, pooled here instead of read
from a single opportunistic run:

| | `main` (3 runs, n=18 walk-seconds pooled) | #718 (3 runs, n=22 walk-seconds pooled) |
|---|---|---|
| Wakeups/sec (interrupt) | 369.1 | 373.4 |
| Wakeups/sec (pkg-idle) | 4.06 | 3.92 |
| CPU% | 16.3 | 16.4 |

No meaningful difference between branches while walking, and both read close
to their own idle-family numbers above — consistent with #718's own design
(a multi-frame animation is Active mode, unchanged 16 ms tick, on both
branches) and with #431's own hypothesis that walking should not read
differently from idle. Unlike the idle-perched table, this is not a claim
about a cross-branch delta; it is pooled n to replace what was previously a
single 5–7 second sample per branch, nothing more.

## Chat-open (single pair, not interleaved, no percentage derived)

Chat-open needs Summon (a driven double-click, sprite-position read, and a
verified trace line per run) rather than a plain launch-and-wait, so it was
not cheap enough to also run three-times-interleaved in this pass. **This
is one capture per branch, not a range, and no delta is derived from it:**

| Branch | n | Wakeups/sec (interrupt) | Wakeups/sec (pkg-idle) | CPU% |
|---|---|---|---|---|
| `main` | 30 | 417.4 | 4.28 | 14.13 |
| #718 | 30 | 416.8 | 3.98 | 15.06 |

**This pair itself already moved once, which is why no percentage is drawn
from it.** A first pass measured `main` at 2.64 pkg-idle wakeups/sec and
#718 at 1.63–2.12. Re-measured about fifteen minutes later, with no code
change other than swapping the click script for #728's shared one (a
one-time ~300 ms event inside a 30-second average — not a plausible cause of
a 60% shift), both branches moved to the ~4/sec figures in the table above.
Whatever moved both branches' chat-open number by the same ~60% in the same
fifteen minutes was the machine, not this document's code, and it is the
same effect the interleaved idle-perched runs were built to control for.
Treat this table as two single samples, not as a measurement of #718's
effect on chat-open.

## What "C-state" means on this hardware

#431 asks for "C3/C6/C7 residency," which is Intel nomenclature. This machine
is Apple Silicon (M3 Pro); `powermetrics --samplers cpu_power` does not report
those states at all. The closest available analog is per-cluster idle
residency, from the same interleaved idle-perched runs above (median
[range] across the three runs per branch):

| | `main` | #718 |
|---|---|---|
| E-Cluster idle residency | 29.9% [16.2–30.8%] | 21.5% [15.2–29.6%] |
| P-Cluster idle residency | 74.8% [74.5–75.8%] | 74.9% [73.7–78.9%] |
| Package CPU power | 644 mW [564–1063 mW] | 856 mW [638–969 mW] |

These ranges overlap even more than the wakeup counts do — P-Cluster idle
residency in particular is essentially identical between branches. Read that
as the same shared-machine confound as above, not as evidence #718 has no
effect on idle residency: these are system-wide numbers, so anything else
running on the machine during a given round shows up here as strongly as
`ai-buddy` does, or more. The per-process wakeup counts are the more
trustworthy comparison in this document; these cluster-residency and
package-power numbers are included for completeness and are not a second
confirmation of anything.

## Gotchas

- **Filter by PID, never by process name alone.** `powermetrics` is
  system-wide. During this task a second, unrelated `ai-buddy` process
  (another agent's own build) appeared in a real capture under a different
  PID. `parse-powermetrics.py --process ai-buddy` without `--pid` will warn
  and silently pick whichever line comes first in that sample — always pass
  `--pid`, which `bench-wakeups-macos.sh` prints on every run.
- **A single capture per branch is not a measurement, it is a sample.** The
  first draft of this document reported one 60-second run per branch as a
  65–75% reduction. Interleaved repetition (three runs per branch, A/B
  alternating) showed the true per-branch spread is wider than that gap.
  Do not trust a single A-vs-B comparison on this machine; interleave and
  report medians and ranges, or say explicitly that a pair is a single
  sample and derive no percentage from it (see Chat-open).
- **`osascript ... click at` does not work on this overlay.** See Tools;
  use `click-cursor.swift`, and use the shared one (#728's), not a fork of
  it — two scripts at the same path is a silent collision, not a merge
  conflict git will catch.
- **The sprite falls before it lands.** Compute the click point only after a
  `Grounded`/`Perched` frame line, not the first `frame:` line seen.
- **`sudo powermetrics` needs non-interactive sudo.** This environment had
  it (`sudo -n true` succeeded with no prompt). Where it does not, these
  captures cannot run unattended; there is no non-root substitute for
  per-process wakeup counts on macOS that this task found.
- **This machine is not quiet, and the numbers say so.** See Headline. Do
  not read a single capture as a stable constant; a rerun on a quiet
  machine is expected to disagree.

## Not measured

Scoped out per this task's instructions, not fabricated:

- **Multi-monitor** — #424's scope, not this issue's.
- **Baseline (no ai-buddy running)** and **hidden/fullscreen** — #431's own
  issue body lists six scenarios; the task that produced this document
  narrowed to the three #423 names for every child (idle perched, walking,
  chat open). Baseline and hidden are not in this document; a follow-up
  using the same `scripts/bench-wakeups-macos.sh` needs only a new
  `--scenario` case (hidden: launch, then trigger the fullscreen-hides rule;
  baseline: `sudo powermetrics` with no ai-buddy process running at all).
- **Linux and Windows** — #431 is macOS-only (#432 covers Linux).
- **Chat-open, interleaved** — left as a single pair per branch; see
  Chat-open above for why and for the explicit no-percentage rule that
  applies to it.
- **A tighter idle-perched effect size** — three interleaved runs per branch
  established direction, not a resolved magnitude (see Headline and the
  idle-perched table). More runs, or a quieter machine, would narrow the
  range; this document stops at the n the task asked for.
- **What actually produces the ~300–450 wakeups/sec baseline** — not
  isolated. See Headline. WKWebView's event pump, the async runtime, log
  rotation, and the tray icon are named as candidates because they are the
  process's other standing activity, not because any one of them was
  measured separately from the others.
