# Memory (RSS) and multi-monitor scaling

Baseline for #424, under #423's plan. This document covers macOS (measured),
Linux (method only), and Windows (method only). Each platform has different
overlay toolkits and process architectures, so numbers are not comparable across
operating systems.

## Summary of macOS findings

A two-display, one-Instance buddy peaks at **583 MB** of physical footprint
across five processes, and read a median **259 MB** resident in a window with a
third of the machine's memory free. Footprint scales with **pixels**, not with
Instances: each display is one `WebContent` process, and the 3456×2234 panel's
is 75–110 MB heavier than the 1920×1080 one in every run. Four Instances cost
**+56 MB**, all of it inside the webviews — the Rust process's peak did not move
at all. Character art is not what makes a buddy large: all eight installed
packages are 4.5 MB of base64, and because the app loads every one of them at
launch, switching Character cannot make RSS grow. What the sprite is doing does
not move the number either: idle, walking, sitting and talking read within a few
MB of each other.

---

## macOS

### The machine and the build

| | |
|---|---|
| Machine | MacBook Pro `Mac15,7`, 36 GB, macOS 26.6.2 (25G83) |
| Display 1 | DELL P2414H, 1920×1080 at 1×, main, overlay at (0,0) |
| Display 2 | Built-in Liquid Retina XDR, 3456×2234 at 2× — 1728×1117 in points, overlay at (1920,0) |
| Build | `target/debug/ai-buddy`, **debug**, ad-hoc signed at a worktree path |
| Completer | none — `AI_BUDDY_DIRECTOR=0`, so `StaticDirector` picks every Behavior and no HTTP leaves the process |
| Date | 2026-09-09 |

The debug build is what #424 asked to run, and it is not what a user runs. Read
the Rust process's share as an upper bound; the WebKit helpers are release code
either way and are unaffected by it.

### How to reproduce

`scripts/bench-rss.sh` launches the app, waits out the settling curve below,
samples every process's RSS on a fixed interval, and reads each one's peak
physical footprint before it stops the app.

**Default: Brief smoke test** (settle ~3s, sample ~10s) — fast enough for a test
matrix with many scenarios. Not a research soak.

**Research mode: `--research`** restores the long settle + sample (300s each)
for bathtub curve analysis and measurement studies.

```sh
cd src-tauri && cargo build --bin ai-buddy && cd ..

HOME=/tmp/bench-home \
AI_BUDDY_DIRECTOR=0 AI_BUDDY_DIRECTOR_API_KEY=x AI_BUDDY_CAPTURABLE=1 \
AI_BUDDY_TRACE_ENGINE=1 AI_BUDDY_CHARACTERS="$PWD/characters" \
AI_BUDDY_INSTANCES="bmo:One" \
scripts/bench-rss.sh --out /tmp/one-instance.tsv

# Research mode (300s settle + 300s sample) for measurement studies:
scripts/bench-rss.sh --research --out /tmp/research-run.tsv
```

Five variables carry the run. `AI_BUDDY_INSTANCES` is the roster,
`AI_BUDDY_CHARACTERS` the set of installed packages, `AI_BUDDY_DIRECTOR=0` keeps
the network out of it, and `HOME` points at a scratch directory so nothing
touches the real install's settings or Action Log. `AI_BUDDY_DIRECTOR_API_KEY`
is not a credential here: a worktree build is a new path to the Keychain, and
without the variable the launch stops on a password dialog (#283, #290).

`AI_BUDDY_TRACE_ENGINE=1` is what makes a run reportable rather than a number.
An RSS figure with no record of what the sprite was doing is not a measurement,
and the trace is the only thing that says.

### Where the memory is

WKWebView runs its content out of process, and those processes are children of
`launchd`, not of the app — no process-tree walk finds them.

```
ai-buddy                       the Rust binary: Engine, frame loop, art, Tauri
com.apple.WebKit.WebContent    one per overlay, so one per display
com.apple.WebKit.WebContent
com.apple.WebKit.GPU           one, shared
com.apple.WebKit.Networking    one, shared
```

`ps -o rss= -p <app>` reports a third of the truth. The script takes the set of
WebKit helpers before launch and again just after it, and calls the difference
the app's — which is also its one soft spot, since another application starting
a helper inside that window lands in the set. The script warns when the count is
not `displays + 2`; a run that warns is contaminated and should be repeated.

### The launch peak is not the number

RSS after launch is a bathtub, not a plateau. Fifteen minutes, one Instance, two
displays, idle desktop — an earlier run than the four below, and the only one
with a Completer configured, which is why its resting level is not theirs:

| Since launch | Total RSS |
|---|---|
| 0 s | 396 MB |
| 60 s | 272 MB |
| 120 s | 205 MB |
| 150 s | 178 MB |
| 300 s | 224 MB |
| 450 s | 228 MB |
| 600 s | 245 MB |
| 750 s | 265 MB |
| 900 s | 264 MB |

The first minute is the loader's. The dip near 150 s is macOS reclaiming pages
the launch faulted in and never touched again. Everything below settles
for 300 s before it samples, for that reason.

The slow climb out of that dip — roughly +1 MB/min, still rising at fifteen
minutes — is not explained here. It is small enough to be page-in of memory the
dip compressed away, and large enough that a multi-hour soak is
worth running before anyone calls it a plateau. **Open question, not a finding.**

### What RSS on macOS does and does not mean

RSS is what the kernel has let a process keep, not what it needs. A busy machine
takes pages back from an idle buddy, and the same app then reads far lighter for
reasons that have nothing to do with the app. That is not a small effect here —
it is larger than every difference #424 asks about:

| Run | Free memory during sampling | Median RSS |
|---|---|---|
| A — 1 Instance, 8 Characters installed | 33–43 % | 259 MB |
| B — 4 Instances of one Character | 25–29 % | 226 MB |
| C — 4 Instances, 4 Characters | 37–42 % | 288 MB |
| D — 1 Instance, 1 Character installed | 43 % | 309 MB |

Read down that column and four Instances look *cheaper* than one, and one
installed Character looks dearer than eight. Both are artifacts of when each run
happened to be sampled. This machine was shared with other agents building and
running throughout, and their load is the free-memory column.

Peak physical footprint — Activity Monitor's "Memory", what `vmmap` reports —
only ever rises, so it survives a noisy machine. Every comparison below is made
on the footprint. The RSS series is kept for shape, not for ranking.

### Results

Peak physical footprint per process, MB, over a 300 s settle plus 300 s sample:

| Run | `ai-buddy` | GPU | Networking | 1920×1080 overlay | 3456×2234 overlay | **Total** |
|---|---|---|---|---|---|---|
| A — 1 Instance, 8 Characters installed | 82.9 | 146.3 | 8.0 | 132.7 | 213.2 | **583** |
| B — 4 Instances of one Character | 82.9 | 147.6 | 7.3 | 162.0 | 239.5 | **639** |
| C — 4 Instances, 4 Characters | 85.1 | 150.2 | 7.9 | 170.9 | 280.5 | **695** |
| D — 1 Instance, 1 Character installed | 35.3 | 139.8 | 8.4 | 122.6 | 197.7 | **504** |

Which `WebContent` column is which display is not a guess: `place_overlays`
builds overlay-0 over the main display first, so its helper takes the lower pid,
and in all four runs the lower pid is also the lighter process.

### Displays cost pixels, not displays

#424's hypothesis is that RSS scales linearly with display count. It scales with
display *area*. The two overlays are the same document, the same sprite and the
same art, and one is consistently far heavier than the other:

| Run | 1920×1080 overlay | 3456×2234 overlay | Difference |
|---|---|---|---|
| A | 132.7 | 213.2 | +80.5 |
| B | 162.0 | 239.5 | +77.5 |
| C | 170.9 | 280.5 | +109.6 |
| D | 122.6 | 197.7 | +75.1 |

The ratio of backing stores is the ratio of pixels: 3456 × 2234 × 4 B is 30.9 MB
a buffer against 1920 × 1080 × 4 B's 8.3 MB, and the gap above is two to three
buffers' worth. So the planning number is not "N displays × 150 MB". It is
roughly **120–170 MB for a 1080p display and 200–280 MB for a Retina one**, and
a 5K panel should be expected to cost more again.

This is measured from the process split of a two-display configuration, not by
comparing one display against two: both are permanently attached to this machine
and unplugging one is not something an agent can do. The split is sound — the
`WebContent` processes exist one per overlay, and `place_overlays` closes an
overlay when its display goes away — but a single-display run would still be
worth taking on a machine where it is possible.

### Instances cost webview, not engine

Four Instances of one Character against one Instance (B against A) is **+56 MB**,
and every MB of it is in the two `WebContent` processes: +29 and +26. The Rust
process's peak was **82.9 MB in both runs, to the tenth of a MB.** The Engine's
per-Instance state and the frame it emits are nothing next to four `<img>`
elements and four bubble layers per overlay.

Making those four Instances four *different* Characters (C against B) is another
**+56 MB**, again nearly all in the webviews (+9 and +41) with the Rust side
moving 2.2 MB. Four Characters on screen means four sprite sheets actually
decoded by WebKit, where four of one Character means one.

### Character art, and why a switch cannot leak

Every installed package's art is decoded, base64-encoded and held for the life
of the process — `load_all_characters` runs at launch, not on demand, so that a
switch or a spawn does not wait for a load the overlay never does. The art is
keyed by Character rather than by Instance, so two Instances of one Character
share one copy.

| Character | Frames | PNG | as base64 `data:` URLs |
|---|---|---|---|
| black-mage | 25 | 20 KB | 27 KB |
| bmo | 33 | 222 KB | 296 KB |
| buddy-bot | 78 | 575 KB | 767 KB |
| cat | 45 | 720 KB | 960 KB |
| jotaro-kujo | 38 | 538 KB | 717 KB |
| nim | 44 | 18 KB | 23 KB |
| timber-wolf | 41 | 664 KB | 885 KB |
| trump | 38 | 635 KB | 846 KB |
| **all eight** | **342** | **3.3 MB** | **4.5 MB** |

Two answers #424 asks for:

- **A Character switch cannot grow RSS permanently.** The art switched to is
  already resident and the art left behind is never freed, so there is nothing
  to accumulate. The cost was paid at launch. This is a claim about the code
  path, checked against it, not a measured switch — driving the tray menu is a
  human step.
- **Seven unused installed Characters cost 79 MB of peak footprint** (A against
  D), 47.6 MB of it in the Rust process. That is ten times the 4.5 MB of base64
  they amount to. The likely reason is that the peak is not the resting size:
  `art_urls` allocates a fresh `String` per animation frame, and the `character`
  command serializes the whole map to JSON once per overlay. Neither is
  confirmed — **a heap profile would settle it, and this document has none.**

### What the sprite was doing

`StaticDirector` picks ambient Behaviors, so a run is a mix rather than a held
pose. Run A's 300 s window: idle 59 %, walk 22 %, sit 8 %, talk 5 %, climb 4 %,
the rest fall/land/react.

Matching each RSS sample to the animation live at that instant:

| Animation | Samples | Median RSS |
|---|---|---|
| idle | 37 | 260 MB |
| walk | 12 | 245 MB |
| sit | 5 | 260 MB |
| talk | 2 | 260 MB |
| climb | 2 | 240 MB |

The spread across animations is smaller than the spread within any one of them.
**Idle perched and walking cost the same memory**, which is the useful negative
result: whatever the frame loop is costing (#431), it is not costing pages. The
other three runs agree — B reads 226 MB idle and 226 MB walking.

### Not measured (macOS)

- **A single-display comparison, and three displays.** Both displays are
  permanently attached and no third exists. See the per-display section for what
  stands in.
- **A heap profile with top allocators.** #424 asks for Instruments Allocations
  or `heaptrack`. Instruments needs a GUI session and a human. The allocator
  ranking is unanswered, and the 47.6 MB above is the first thing to point it at.
- **A release build.** Everything here is `target/debug`.
- **Chat windows open.** Every run is overlays only.

---

## Linux

**Status: Method provided, measurement completed on Grok Bot box.**

### Results

One successful unattended run on Grok Bot box after the stderr-file fix
(commit f27703b):

| | |
|---|---|
| Machine | Grok Bot box |
| Display | 1 (DISPLAY=:3) |
| Scenario | 1 Instance, `bmo:One` |
| Settle | 5s |
| Sample duration | 30s |
| Sample interval | 2s |
| Samples | 15 |
| **Total RSS** | **min 821 MB / median 823 MB / max 933 MB** |
| Process count | 4 (ai-buddy + WebKitNetworkProcess + 2× WebKitWebProcess) |
| Exit code | 0 |

Per-process medians and peak RSS (VmHWM): ai-buddy 237/250 MB,
WebKitNetworkProcess 61/60.5 MB, WebKitWebProcess 238/238 MB,
WebKitWebProcess 287/396 MB.

Total RSS includes the main ai-buddy process plus all WebKitGTK helper processes.

### How to run (on a machine with a display)

`scripts/bench-rss-linux.sh` implements the same contract as the macOS script,
adapted for Linux:

**Default: Brief smoke test** (settle ~3s, sample ~10s) — fast enough for a test
matrix with many scenarios. Not a research soak.

**Research mode: `--research`** restores the long settle + sample (300s each)
for bathtub curve analysis and measurement studies.

```sh
cd src-tauri && cargo build --bin ai-buddy && cd ..

HOME=/tmp/bench-home \
AI_BUDDY_DIRECTOR=0 AI_BUDDY_DIRECTOR_API_KEY=x AI_BUDDY_CAPTURABLE=1 \
AI_BUDDY_TRACE_ENGINE=1 AI_BUDDY_CHARACTERS="$PWD/characters" \
AI_BUDDY_INSTANCES="bmo:One" \
scripts/bench-rss-linux.sh --out /tmp/one-instance.tsv

# Research mode (300s settle + 300s sample) for measurement studies:
scripts/bench-rss-linux.sh --research --out /tmp/research-run.tsv
```

### Process architecture

WebKitGTK's process model depends on version and build configuration:

- **Modern WebKitGTK (2.26+)** uses a multi-process architecture similar to
  macOS: separate processes for WebContent, GPU, Network.
- **Older or sandboxing-disabled builds** may run everything in-process.

The Linux script uses `pgrep -P <pid>` to find all child processes of the main
ai-buddy process. WebKitGTK helpers on Linux are children of the main process
(unlike macOS where they are children of `launchd`), so the process tree walk
discovers them automatically.

### Measurement method

- **RSS:** Read from `/proc/[pid]/status` field `VmRSS` (current resident set).
- **Peak RSS:** Read from `/proc/[pid]/status` field `VmHWM` (high-water mark).
  This is the Linux equivalent of macOS's peak physical footprint and only ever
  rises, so it survives a noisy machine.
- **Settling:** Defaults to 3s for brief smoke tests. Use `--research` (or
  explicit `--settle 300 --seconds 300`) for the full settling curve measured
  on macOS. The settling curve should be measured independently on Linux to
  validate or adjust this.

### Display count

Run `xrandr` or check the app's log for `overlay: N display` to determine how
many displays the app detected. If WebKitGTK runs one WebContent process per
overlay (as WebKit does on macOS), the per-display cost will be visible in the
process split.

### Expected behavior (based on macOS findings)

Based on the macOS findings (not yet validated at scale on Linux):

- **Displays cost pixels, not count.** A 1920×1080 overlay may be 100–170 MB
  while a 2560×1440 overlay may be 200–300 MB, depending on WebKitGTK's backing
  store implementation.
- **Instances cost webview, not engine.** Multiple Character Instances should
  add cost to the WebContent processes, not the main Rust process.
- **Character art is front-loaded.** All installed Characters are loaded at
  launch, so a Character switch cannot grow RSS permanently.

### Heap profiling (optional)

If heap profiling is desired, use `heaptrack` on Linux:

```sh
heaptrack target/debug/ai-buddy
# ... run the scenario ...
heaptrack --analyze heaptrack.ai-buddy.*.gz
```

Look for top allocators and whether unused Character art (base64 strings) is
the 47.6 MB gap found on macOS.

---

## Windows

**Status: Method provided, measurement completed on DESKTOP-UQIE144.**

### Results

One successful unattended run on DESKTOP-UQIE144 after the stderr-file fix
(commit f27703b):

| | |
|---|---|
| Machine | DESKTOP-UQIE144 |
| Displays | 2 (3440×1440 + 1200×1920) |
| Scenario | 1 Instance, `bmo:One` |
| Settle | 5s |
| Sample duration | 30s |
| Sample interval | 2s |
| **Total working set** | **min 607 MB / median 612 MB / max 620 MB** |
| Exit code | 0 |

Total working set includes ai-buddy.exe plus all msedgewebview2.exe helper
processes.

### How to run

`scripts\bench-rss.ps1` implements the same contract as the macOS and Linux
scripts, adapted for Windows PowerShell:

**Default: Brief smoke test** (settle ~3s, sample ~10s) — fast enough for a test
matrix with many scenarios. Not a research soak.

**Research mode: `-Research`** restores the long settle + sample (300s each) for
bathtub curve analysis and measurement studies.

```powershell
cd src-tauri
cargo build --bin ai-buddy
cd ..

$env:HOME = "C:\Temp\bench-home"
$env:AI_BUDDY_DIRECTOR = "0"
$env:AI_BUDDY_DIRECTOR_API_KEY = "x"
$env:AI_BUDDY_CAPTURABLE = "1"
$env:AI_BUDDY_TRACE_ENGINE = "1"
$env:AI_BUDDY_CHARACTERS = (Get-Location).Path + "\characters"
$env:AI_BUDDY_INSTANCES = "bmo:One"

.\scripts\bench-rss.ps1 -Out "C:\Temp\one-instance.tsv"

# Research mode (300s settle + 300s sample) for measurement studies:
.\scripts\bench-rss.ps1 -Research -Out "C:\Temp\research-run.tsv"
```

### Process architecture

WebView2 on Windows uses the Chromium (Edge) multi-process architecture:

- **Main process:** `ai-buddy.exe` (the Rust binary).
- **WebView2 helpers:** Multiple `msedgewebview2.exe` processes:
  - **Renderer:** One per webview (so one per display for the overlay).
  - **GPU process:** Shared.
  - **Network service:** Shared.
  - **Utility processes:** Various (audio, storage, etc.).

The Windows script uses `Get-Process -Name "msedgewebview2"` before and after
launch to find all WebView2 helpers that appeared. These are not child processes
of ai-buddy.exe (they are children of the Edge browser infrastructure), so the
script uses a set-difference approach similar to the macOS WebKit helper
discovery.

### Measurement method

- **Working Set:** Current memory usage from `Get-Process | Select-Object
  WorkingSet64`. This is the Windows equivalent of RSS.
- **Peak Working Set:** Read from `Get-Process | Select-Object
  PeakWorkingSet64`. This only ever rises, so it survives a noisy machine.
- **Settling:** Defaults to 3s for brief smoke tests. Use `-Research` (or
  explicit `-Settle 300 -Seconds 300`) for the full settling curve measured on
  macOS. The Windows measurement above used 5s settle / 30s sample for a quick
  validation run.

### Display count

Check the app's log for `overlay: N display` to determine how many displays the
app detected. Windows has a Renderer process per webview, so the per-display
cost will be visible in the process split.

### Expected behavior (based on macOS findings)

Based on the macOS findings (not yet validated at scale on Windows):

- **Displays cost pixels, not count.** WebView2's Renderer process memory usage
  should scale with overlay resolution (backing store size).
- **Instances cost webview, not engine.** Multiple Character Instances should
  add cost to the Renderer processes, not the main ai-buddy.exe process.
- **Character art is front-loaded.** All installed Characters are loaded at
  launch, so a Character switch cannot grow memory permanently.

### Machine details

The successful measurement above ran on DESKTOP-UQIE144 after commit f27703b (the
stderr-file fix). The script completed unattended with exit code 0.

### Heap profiling (optional)

If heap profiling is desired on Windows, use Windows Performance Analyzer (WPA)
or the Visual Studio profiler:

```powershell
# Using Windows Performance Recorder (WPR)
wpr -start GeneralProfile -filemode
# ... run ai-buddy ...
wpr -stop profile.etl
# Analyze with Windows Performance Analyzer (wpa.exe profile.etl)
```

Look for top allocators in the Rust process and whether unused Character art
accounts for the gap found on macOS.
