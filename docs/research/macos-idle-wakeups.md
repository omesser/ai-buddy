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
this process sits at **~400–450 wakeups/sec**, on `main` and on #718's head,
in every scenario. The frame loop's 16 ms-vs-1 s tick difference cannot
explain a gap that size. Something else in this process — a WKWebView event
pump, the async runtime, log rotation, the tray icon are candidates, in
descending order of how much of this process's own thread activity they plausibly
own — accounts for the bulk of it. **This is a guess, not a finding**: nothing
in this task isolated which one, or whether it is several of them at once. A
follow-up that wants the real number needs `sample` or a per-thread
breakdown, not `powermetrics --samplers tasks`, which only totals wakeups at
the process level.

**#718's fix is real, on a narrower metric.** powermetrics reports a second,
more specific column: "Wakeups (..., Pkg idle)" — wakeups that happened
while the whole package was otherwise idle, the number that actually answers
"does this process block deep sleep." There, idle-perched `main` measures
**~3.6/sec**; idle-perched #718 measures **~0.8–1.3/sec**, a **65–75%
reduction**. That is #718's claimed win, measured, not inferred from the
diff — and it is a real reduction in the metric #431 actually cared about,
even though the raw wakeup count it led with does not move.

**Read every number below against a shared, noisy machine, not a clean-room
rig.** This is a dev laptop with other agents building and running their own
work on it throughout every capture in this document, including their own
`ai-buddy` instances. That is not a hypothetical caveat: re-measuring the
chat-open scenario about fifteen minutes after the first attempt (below)
moved that scenario's package-idle wakeups by ~60% on **both** branches, with
no code change and the same click mechanism in every meaningful respect. A
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
| `main` | `7a58e02f` (this issue's base) |
| PR #718 head | `9864d789` (`cursor/macos-idle-backoff-183-3b49`) |

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
Headline above for a measured example of exactly that. The `cpu_power`
sampler's idle-residency and package-power numbers are **system-wide**, not
per-process, so they carry the same confound a second time, more directly.

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

## Results

30-second samples unless noted; idle-perched samples are 60 seconds so a
natural ambient walk (BMO's `StaticDirector` picks `walk`-containing
Behaviors on its own, no synthetic trigger — see Gotchas) has a chance to
land inside the same capture. All numbers are **measured** with the tools
above; sample counts (`n`) are seconds of powermetrics data in each bucket.
**Read these against the Headline section above: this is a shared machine,
and the chat-open row for both branches was re-measured once already,
fifteen minutes apart, and moved by ~60% with no code change (see note under
that row).**

### `main` (7a58e02f)

| Scenario | n | Wakeups/sec (interrupt) | Wakeups/sec (pkg-idle) | CPU% |
|---|---|---|---|---|
| Idle perched | 52 | 396.9 | 3.59 | 9.98 |
| Walking (natural, within the idle capture) | 5 | 450.8 | 2.56 | 8.79 |
| Chat open (Summon confirmed) | 30 | 417.4 | 4.28 | 14.13 |

### PR #718 head (9864d789)

| Scenario | n | Wakeups/sec (interrupt) | Wakeups/sec (pkg-idle) | CPU% |
|---|---|---|---|---|
| Idle perched (`anim=idle` only) | 42 | 402.0 | 0.84 | 7.05 |
| Idle perched (idle-family: idle+sit) | 49 | 402.6 | 1.28 | 6.85 |
| Walking (natural, within the idle capture) | 6 | 437.6 | 1.80 | 8.03 |
| Chat open (Summon confirmed) | 30 | 416.8 | 3.98 | 15.06 |

**Chat-open moved between runs, and that is the finding, not noise to
average away.** A first pass measured `main` at 2.64 pkg-idle wakeups/sec and
#718 at 1.63–2.12 — a gap that read like idle-perched's. Re-measured about
fifteen minutes later, with no code change other than swapping the click
script for #728's shared one (a one-time ~300 ms event inside a 30-second
average — not a plausible cause of a 60% shift), both branches now read
close to **4/sec**, `main` at 4.28 and #718 at 3.98: barely different from
each other. The idle-perched gap (3.59 vs. 0.8–1.3) did not get re-measured
and stands as originally captured; only chat-open used the now-shared click
script, so only chat-open was re-run. Whatever moved both branches' chat-open
number by the same ~60% in the same fifteen minutes was not this document's
code — it was the machine. Treat the chat-open numbers as a demonstration
that this metric is sensitive to concurrent load at least as much as it is
to which branch is running, not as a clean confirmation or refutation of
#718 for that scenario.

### The delta (idle-perched only — the one scenario not re-measured mid-task)

| | `main` idle | #718 idle | Change |
|---|---|---|---|
| Wakeups/sec (pkg-idle) | 3.59 | 0.84–1.28 | **−64% to −77%** |
| Wakeups/sec (interrupt) | 396.9 | 402.0–402.6 | ~flat |
| CPU% | 9.98 | 6.85–7.05 | −29% to −31% |

Package-idle-pulling wakeups are the metric #431's hypothesis actually cared
about (it is the proxy for "prevents deep sleep"), and #718 cuts it by
roughly two thirds to three quarters while idle and visible. Raw interrupt
wakeups do not move because they are dominated by something other than the
frame loop's own tick (see Headline). CPU% drops by about three points,
consistent with fewer scheduler wake-ups on the frame-loop thread, but small
next to the process's other overhead.

## What "C-state" means on this hardware

#431 asks for "C3/C6/C7 residency," which is Intel nomenclature. This machine
is Apple Silicon (M3 Pro); `powermetrics --samplers cpu_power` does not report
those states at all. The closest available analog is per-cluster idle
residency:

| | `main` idle | #718 idle |
|---|---|---|
| E-Cluster idle residency | 21.7% | 14.8%–15.4% |
| P-Cluster idle residency | 44.8% | 27.2%–28.5% |
| Package CPU power | 4000 mW | 4821–5333 mW |

Read these against the shared-machine caveat above — they moved in the
opposite direction from what the pkg-idle wakeup count predicts (lower
residency, more power, on the branch with fewer wakeups), which is the
signature of system-wide noise (concurrent Cargo builds from other agents
during the #718 capture) rather than of #718 doing something wrong. The
per-process wakeup counts above are the trustworthy comparison; these
cluster-residency and package-power numbers are included for completeness
and labeled as confounded, not as a second confirmation.

## Gotchas

- **Filter by PID, never by process name alone.** `powermetrics` is
  system-wide. During this task a second, unrelated `ai-buddy` process
  (another agent's own build) appeared in a real capture under a different
  PID. `parse-powermetrics.py --process ai-buddy` without `--pid` will warn
  and silently pick whichever line comes first in that sample — always pass
  `--pid`, which `bench-wakeups-macos.sh` prints on every run.
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
- **This machine is not quiet, and the numbers say so.** See Headline and
  the chat-open note above. Do not read a single capture as a stable
  constant; a rerun on a quiet machine is expected to disagree.

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
- **Walking, forced** — this harness deliberately never posts a synthetic
  walk trigger; the `walk` numbers above are opportunistic, from
  `StaticDirector` picking a `walk`-containing Behavior on its own during a
  60-second idle-perched capture. Sample sizes for `walk` are small (5–7
  seconds) because of that; treat them as indicative, not as tight as the
  idle numbers.
- **What actually produces the ~400 wakeups/sec baseline** — not isolated.
  See Headline. WKWebView's event pump, the async runtime, log rotation, and
  the tray icon are named as candidates because they are the process's other
  standing activity, not because any one of them was measured separately
  from the others.
