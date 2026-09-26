# Performance Baseline v1

Measured baselines for ai-buddy performance before optimization work. See parent issue [#423](https://github.com/omesser/ai-buddy/issues/423) for context and child benchmarks.

## Linux (issue #432)

**Environment:**
- Ubuntu 24.04.4 LTS (Noble)
- Kernel 6.12.94+ (cloud VM, not bare metal laptop)
- X11 via Xtigervnc (no Wayland)
- No desktop environment (headless VM)
- 4 vCPU Intel Xeon (virtualized)

**Measurement limitations:**
- VM has no real CPU C-states (no laptop power management)
- `powertop` system-wide wakeup measurement unavailable in VM
- No multi-monitor testing (single VNC display)
- Limited GUI interaction testing (headless environment)
- `perf` unavailable for kernel version
- Used context switches from `/proc/<pid>/status` as wakeup proxy

**Tools used:**
- `powertop 2.15` (limited data in VM)
- `/proc/<pid>/status` for context switch counting
- `htop`/`top` for CPU%
- Direct process measurement (60s samples)

**Metrics:**

| Scenario | Wakeups/sec (voluntary ctx switches) | CPU% | Notes |
|----------|--------------------------------------|------|-------|
| Baseline (no ai-buddy) | 3.6 | - | X server (Xtigervnc) idle |
| Idle perched | ~271 | ~3% | Sprite visible, no interaction |
| Walking | N/A | N/A | Wakeups not measured; mask rebuild while walking measured on Grok Bot desktop (see #428 doc) |
| Chat open | N/A | N/A | Not measured (requires GUI interaction) |
| Multi-monitor | N/A | N/A | Not available in VM |
| Hidden | N/A | N/A | Not measured |

**Calculation details:**

Idle perched (60s sample, PID 12927):
- Initial voluntary context switches: 3,556
- After 60s: 19,821
- Rate: (19,821 - 3,556) / 60 = **271 voluntary ctx switches/sec**
- Average CPU: ~3%

Baseline X server (60s sample, PID 1594):
- Initial: 82,558
- After 60s: 82,772
- Rate: (82,772 - 82,558) / 60 = **3.6 ctx switches/sec**

**Findings:**

1. **High idle wakeup rate.** ai-buddy idle shows ~271 wakeups/sec vs baseline 3.6/sec (75x increase). Hypothesis: unconditional frame loop sleep (~16ms = ~60Hz) plus additional subsystem polling.

2. **VM measurement constraints.** C-state residency and system-wide wakeup counting unavailable. Context switches are a coarse proxy. Bare-metal measurements would provide more accurate power impact data.

3. **Comparison to macOS target.** macOS issue [#431](https://github.com/omesser/ai-buddy/issues/431) targets ~60 wakeups/sec idle. Linux VM shows 4.5x higher rate. Unknown how much is VM overhead vs real difference.

4. **Untested scenarios.** Walking, chat open, and window state changes require GUI automation not feasible in headless VM. Multi-monitor testing requires different environment.

**Evidence:**

Startup log excerpt:
```
character: BMO from target/debug/characters/bmo
libEGL warning: DRI3 error: Could not get DRI3 device
window_source: 0 visible windows
overlay: overlay-0 covers 1920x1200 at (0,0)
overlay: 1 display(s); sprite 126x128; BMO as BMO
director: StaticDirector
```

`powertop` output shows minimal data in VM (see issue comment for full CSV).

**Status:** Partial baseline captured. Idle perched wakeup rate measured. Full scenario matrix blocked by VM/headless constraints. Bare-metal Linux desktop testing recommended for complete baseline.

_Measured in cloud agent environment. Real laptop measurements would capture C-state residency and battery impact._

## macOS (issue #431)

_Pending._

## GPU Compositing

### macOS Metal (issue #429)

Re-run with `AI_BUDDY_BENCH_GREEN_LIGHT=1 scripts/bench-gpu-compositing-macos.sh matrix --seconds 15` after `sudo -v`. The script refuses every scenario but `env` and `baseline` without that variable, because the rest launch ai-buddy on the live desktop, warp the cursor, or cover the main display. Written against `79cd3061`.

**Tools:**

- `scripts/bench-gpu-compositing-macos.sh`
- GPU% is `ioreg -c IOAccelerator` `PerformanceStatistics` `Device Utilization %`, sampled once a second, no sudo. VRAM is `In use system memory` from the same dictionary, which on Apple silicon is the GPU's share of unified memory.
- Watts and HW active residency are `sudo powermetrics --samplers gpu_power`, reduced by `scripts/parse-powermetrics.py`.
- Frame rate is N/A. The compositor's presented rate needs Instruments (Metal System Trace). `ticks_hz` counts the engine's `frame:` lines instead, so it says how often the rAF loop ticked, not how often WindowServer composited.
- Chat and hidden reuse `scripts/click-cursor.swift` and `scripts/fullscreen-window.swift` from `scripts/bench-wakeups-macos.sh`.

**Environment:**

- Mac15,7 (Apple M3 Pro, `AGXAcceleratorG15X`), macOS 26.7 (25G229)
- Two displays, 60 Hz

**Metrics:**

| Scenario | GPU% (ioreg) | GPU active% (powermetrics) | Power W | VRAM MB | ticks/s | Notes |
|----------|--------------|----------------------------|---------|---------|---------|-------|
| Baseline (no ai-buddy) | 1.2 | 5.75 | 0.05 | 686 | N/A | 10 s window, `baseline --seconds 10` |
| Idle perched | _Pending._ | | | | | Needs the green light |
| Walking | _Pending._ | | | | | Needs the green light |
| Chat open | _Pending._ | | | | | Needs the green light |
| Multi-monitor | _Pending._ | | | | | Needs the green light |
| Hidden (fullscreen) | _Pending._ | | | | | Needs the green light |

### Windows DWM (issue #430)
_Pending._

### Linux X11/Wayland (issue #425)

Re-run with `scripts/bench-gpu-compositing-linux.sh matrix --seconds 15`. Add `--shot path.png` to grab the GPU tool window during idle perched. The grab stays out of the tree.

**Environment:**

- Ubuntu 24.04.4 LTS
- Kernel 6.12.94+ (cloud VM, Xtigervnc, not a bare-metal GPU)
- X11 on `DISPLAY=:1`. `WAYLAND_DISPLAY` unset. X.Org 21.1.11, vendor The X.Org Foundation
- One screen, 1920×1200, xrandr mode `60.00*+`
- Compositor `xfwm4`, `use_compositing` true, `vblank_mode` auto
- Mutter, KWin, and Xfwm4's uncomposited mode were not running
- Character BMO, 126×128, from `characters/bmo`
- App scenarios ran with `AI_BUDDY_TRACE_FRAMES=1` and `AI_BUDDY_TRACE_MASK_REBUILD=1`

**Measurement limitations:**

- No `/dev/dri` and no `/dev/nvidiactl`. GPU% is N/A on every row.
- `intel_gpu_top` exits with "no discrete/integrated i915 devices found".
- `radeontop` exits with "Failed to find DRM devices" and "Can't find Radeon cards".
- `nvidia-smi` is not installed.
- `glxinfo -B` reports renderer `llvmpipe (LLVM 20.1.2, 256 bits)` and `Accelerated: no`. That is the GL setup check. It is not a utilization percent.
- No Wayland session, so the ADR-0014 degraded lane (X11 does not answer, and the build does not switch protocols) was not exercised. This host is the X11 lane. [ADR-0014](../adr/0014-x11-lane-no-native-wayland.md) is superseded by [ADR-0020](../adr/0020-x11-lane-no-native-wayland.md).
- One display, so multi-monitor is N/A.
- A second compositor was not available. The script records whichever of Mutter, KWin, xfwm4, picom, or Sway is running, and a re-run on that desktop fills the same columns.

**Tools:**

- `intel_gpu_top`, `radeontop`, `glxinfo -B`
- `xfconf-query` for xfwm4 compositing and vblank
- `xrandr` for screen count and refresh
- `mask_rebuild:` lines as the XShapeCombineMask call count. Per-call time stays in the [#428](https://github.com/omesser/ai-buddy/issues/428) study.
- `/proc/<pid>/stat` utime+stime for `xfwm4` and `Xtigervnc`, as percent of one core over the sample window. This is a proxy for where the software composite lands. It is not GPU%.

**Metrics (15s windows, except the aborted pointer-on-sprite window at 5s):**

| Scenario | GPU% | Mask calls | Mask Hz | xfwm4 CPU% | Xtigervnc CPU% | Notes |
|----------|------|------------|---------|------------|----------------|-------|
| Baseline (no ai-buddy) | N/A | N/A | N/A | 0.0 | 0.0 | No client, so no mask caller |
| Idle perched | N/A | 0 | 0.00 | 0.9 | 35.0 | Pointer at (2,2) |
| Walking | N/A | 0 | 0.00 | 1.1 | 46.7 | Pointer away. 457 `walk` frames |
| Pointer on sprite, walk aborted | N/A | 22 | 4.40 | 0.2 | 5.6 | 5s only. 8 `walk` frames, then react and talk. Not a sustained walk rate. That rate is [#428](https://github.com/omesser/ai-buddy/issues/428) |
| Chat open | N/A | 17 | 1.13 | 0.3 | 3.4 | `Summon` logged. Pointer left on the sprite |
| Multi-monitor | N/A | N/A | N/A | N/A | N/A | xrandr reports 1 display |
| Hidden (fullscreen) | N/A | 0 | 0.00 | 0.0 | 0.8 | Log line `presence: hidden over 500ms` |
| Wayland | N/A | N/A | N/A | N/A | N/A | No Wayland display |
| Mutter / KWin | N/A | N/A | N/A | N/A | N/A | Not running |

**Findings:**

1. **GPU% is unread.** The vendor tools exit because the VM has no DRM node. Publishing a 0 here would be a guess. The renderer string is llvmpipe with acceleration off, and the app log repeats the DRI3 failure from the [#432](https://github.com/omesser/ai-buddy/issues/432) run.

2. **X server CPU is the number that moves.** Baseline 0.0%, idle perched 35.0%, walking with the pointer away 46.7%, hidden 0.8%. `xfwm4` stays near 1% or below. Inference from the renderer string: with llvmpipe and no DRM device, that CPU is the software paint of the overlay inside `Xtigervnc`. A bare-metal run with `radeontop`, `intel_gpu_top`, or `nvidia-smi` replaces the N/A column.

3. **XShapeCombineMask stays at 0/s while the pointer is off the sprite.** Idle is 0 calls in 15s. Walking is 0 calls in 15s across 457 walk frames. The walking rate in this run is that 0.00/s. A later 5s window put the pointer on the sprite and the walk aborted. It logged 22 mask calls (4.40/s) and 8 walk frames, then react and talk. 4.40/s is that aborted window, not a sustained walk under the cursor. [#428](https://github.com/omesser/ai-buddy/issues/428) measured 26.7 rebuilds/s when a walk stayed under the cursor. This issue leaves per-call time to that study.

4. **Chat open is a real Summon, with the pointer still on the sprite.** 17 mask calls in 15s (1.13/s). X server CPU in that window is 3.4%. The pointer was not parked away, so the mask rate is the cursor-over rate during chat, and the CPU drop against idle is under that same condition.

5. **Hiding for a fullscreen window returns X server CPU near the baseline.** 0.8% against 0.0% with no client and 35.0% while perched. The compositor flag on xfwm4 stayed on. There is no uncomposited X11 row.

**Evidence:**

The process loaded BMO from the repo path `characters/bmo` (`AI_BUDDY_CHARACTERS=$PWD/characters`). The log named a box-local absolute path, omitted here.

```
libEGL warning: DRI3 error: Could not get DRI3 device
libEGL warning: Ensure your X server supports DRI3 to get accelerated rendering
overlay: 1 display(s); sprite 126x128; BMO as BMO
```

`glxinfo -B` during idle perched: `OpenGL renderer string: llvmpipe (LLVM 20.1.2, 256 bits)`, `Accelerated: no`. The same window shows `intel_gpu_top` and `radeontop` failing for lack of a device.

**Status:** Partial. This section leaves [#425](https://github.com/omesser/ai-buddy/issues/425) open. GPU% per scenario and compositor is still N/A. Two compositors, a Wayland row, and an uncomposited X11 row are still missing. X11 under xfwm4 has a mask-rate pair (idle 0.00/s, walking with the pointer away 0.00/s) and an X-server CPU proxy.

## WindowSource (issue #427)
_Pending._

## Click-through mask (issue #428)

See [mask-rebuild-baseline-x11.md](./mask-rebuild-baseline-x11.md) for detailed X11 measurements.

**Summary (X11 on Grok Bot Linux desktop, 1280×800, tip `6b4acbf`):**
- **Idle perched** (cursor not over sprite): **0.0 rebuilds/sec** (BMO 126×128@1x)
- **Walking under cursor** (BMO on perch): **~26.7 rebuilds/sec**, **~15.0 ms/rebuild** (motion-driven; MaskParams includes x,y)
- **Fast animation** (BMO react, 10 fps): **~9.6 ms/rebuild** (6360 opaque pixels) — FPS doesn't increase rebuild cost, which remains driven by opaque count
- **Large sprite** (Black Mage 37×33@3x): **~1.4 ms/rebuild** (477–579 opaque pixels) — much faster than BMO@1x despite 3× scale, because fewer source opaque pixels. Scale multiplies rendered size, not source opaque count.
- Small sprite: No genuinely smaller shipped character at scale=1; scenario dropped.
- Prior cloud-VM idle: 0.05/sec, 11–13 ms (kept for comparison in detailed doc)
- **Windows:** Not measured (pending DESKTOP approval)
- **`perf` flamegraph:** Not yet collected

## Memory & multi-monitor (issue #424)
_Pending._

## Frame cadence (issue #426)
_Pending._
