# Windows WebView2 anchor RSS measurement

Measurement plan for issue #1218: whether the Windows WebView2 taskbar Settings anchor costs RSS the same way the Linux WebKit anchor did before #1212.

## Method

Compare two release-build runs on the same Windows machine using `scripts\bench-rss-windows.ps1`:

1. **Baseline** (anchor present): Normal run with the WebView2 taskbar anchor
2. **Control** (anchor disabled): Run with `FIDGET_NO_ANCHOR=1` to skip building the anchor

The baseline matches the short scenario from the research doc: one Instance (`bmo:One`), release build, 5s settle, 30s sample, 2s interval. Same scenario that measured the Linux cut in #1212.

## The bar

Linux #1212 dropped 273 MB summed RSS (median 858 → 585 MB) and 1 process (4 → 3). That was the WebKit anchor webview. The Windows WebView2 anchor is the same design: a 1×1 WebView2 window parked off-screen, only opening Settings on a click.

WebView2 on Windows is multi-process (Chromium-based), and the research doc shows msedgewebview2.exe helpers. The bench script counts every helper that appears after launch. If the anchor costs a process, the control run will show one fewer.

A "real hit" means a drop in the same band as Linux: ~200+ MB and 1 fewer process. A smaller drop (e.g., 20-30 MB with no process change) would not clear the bar for a native rewrite.

## How to run

These commands run on Windows PowerShell 5.1 or newer on DESKTOP-UQIE144.

### Baseline (anchor present)

```powershell
cd E:\workspace\fidget
cd src-tauri
cargo build --release --bin fidget
cd ..

$env:HOME = "C:\Temp\bench-home-baseline"
$env:FIDGET_DIRECTOR = "0"
$env:FIDGET_DIRECTOR_API_KEY = "x"
$env:FIDGET_CAPTURABLE = "1"
$env:FIDGET_TRACE_ENGINE = "1"
$env:FIDGET_CHARACTERS = (Get-Location).Path + "\characters"
$env:FIDGET_INSTANCES = "bmo:One"

.\scripts\bench-rss-windows.ps1 -Settle 5 -Seconds 30 -Interval 2 -Out "C:\Temp\baseline.tsv"
```

The script reports total working set (min, median, max) and per-process RSS median and peak working set. Note the process count and the median.

### Control (anchor disabled)

```powershell
# Same setup as baseline, but add FIDGET_NO_ANCHOR=1
$env:HOME = "C:\Temp\bench-home-control"
$env:FIDGET_DIRECTOR = "0"
$env:FIDGET_DIRECTOR_API_KEY = "x"
$env:FIDGET_CAPTURABLE = "1"
$env:FIDGET_TRACE_ENGINE = "1"
$env:FIDGET_CHARACTERS = (Get-Location).Path + "\characters"
$env:FIDGET_INSTANCES = "bmo:One"
$env:FIDGET_NO_ANCHOR = "1"

.\scripts\bench-rss-windows.ps1 -Settle 5 -Seconds 30 -Interval 2 -Out "C:\Temp\control.tsv"
```

The control run should print `anchor: skipped (FIDGET_NO_ANCHOR is set)` to stderr instead of the normal `anchor: 1x1 at ...` line.

## Expected output

Each run prints:

- Display count
- Main pid and total process count
- List of all pids
- Per-process working set at launch
- After sampling: total min/median/max and per-process RSS median and peak working set

Compare:

- **Process count**: baseline vs control (expect control to be 1 fewer if the anchor costs a process)
- **Total working set median**: baseline vs control (expect control to drop if the anchor costs RSS)

## Decision

If the control run shows:
- 1 fewer process, and
- ~200+ MB drop in total working set median,

then the Windows WebView2 anchor costs enough to warrant a native follow-up issue (mirroring the Linux GTK solution).

If the drop is small (<50 MB) with no process change, the WebView2 path is fine and no native rewrite is needed.

Gray zone (50-200 MB drop or 1 process but <50 MB): discuss with Oded.

## Results (DESKTOP-UQIE144, 2026-10-03)

Measured tip `7190cd7c824f7329f4af5e1f8f1ead4648558076` (release fidget.exe, copied onto `target/debug/fidget.exe` because `scripts/bench-rss-windows.ps1` launches that path). One instance `bmo:One`, settle 5s, sample 30s, interval 2s. Homes: `C:/Temp/bench-home-baseline` and `C:/Temp/bench-home-control`. TSV: `C:/Temp/baseline-1218.tsv`, `C:/Temp/control-1218.tsv`.

| run | processes | total WS median | min | max |
| --- | ---: | ---: | ---: | ---: |
| baseline (anchor on) | 9 (1 fidget + 8 msedgewebview2) | 567 MB | 560 MB | 582 MB |
| control (`FIDGET_NO_ANCHOR=1`) | 8 (1 fidget + 7 msedgewebview2) | 483 MB | 479 MB | 498 MB |

Control stderr included `anchor: skipped (FIDGET_NO_ANCHOR is set)`. Windows does not log a success line when the anchor is built; the extra WebView2 process is the baseline signal. Fidget itself stayed about 79 MB median in both runs. The process that disappeared was one msedgewebview2 at about 67 MB RSS median.

Delta: **1 fewer process, 84 MB** (567 to 483).

**Decision: gray.** The process drop matches an anchor WebView2, but 84 MB is inside the 50-200 MB gray band and well under the Linux #1212 bar (about 200+ MB and 1 process). Not a go for a native Windows button on that bar, and not a clean no-go either. Discuss with Oded before filing a native follow-up.

## Related

- #645: parent perf investigation
- #1212: Linux GTK anchor (before/after numbers and method)
- #1218: this issue
