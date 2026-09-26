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
_Pending._

### Windows DWM (issue #430)

The re-run command on DESKTOP-UQIE144 is `powershell -NoProfile -File scripts\bench-gpu-compositing-windows.ps1 matrix --seconds 15`. The script waits for an explicit green light before it launches ai-buddy or moves the cursor. The primary DESKTOP table is the clean run after that green light, with MechWarrior Online closed.

During idle perched, open Task Manager on the Performance GPU page and crop Desktop Window Manager's 3D engine to about 280px wide. Attach that crop with `gh pr comment --attach` in `file#alt` form. The script does not write the image. The image does not belong in the tree.

The host is the workstation named in [mask-rebuild-baseline-windows.md](./mask-rebuild-baseline-windows.md). The Linux VM block is the script check. The primary DESKTOP table is the clean run. The MechWarrior Online pass is an appendix. Its 14% and 56.74 W sample is not ai-buddy's GPU%.

**Environment:**

- Ubuntu 24.04.4 LTS
- Kernel 6.12.94+ on a cloud VM
- The bench printed `windows=False`, `nvidia_smi=absent`, `xperf=absent`, `wpr=absent`, `dwm_pid=N/A`, and `screens=N/A`
- No `ai-buddy.exe` and no Desktop Window Manager

**Measurement limitations:**

- GPU% is N/A. The host is not Windows, so the GPU Engine counter and `dwm.exe` are absent. `nvidia-smi` is not on PATH.
- Power is N/A. The power reading is `nvidia-smi` `power.draw`, and that tool is absent.
- xperf frame time is N/A. `xperf` and `wpr` are not on PATH. When either tool is present, the script still leaves this column N/A. It does not decode an ETL into a frame time.
- Mask cells are N/A. `SetWindowRgn` is not on this host. Rates already published for DESKTOP-UQIE144 stay in the [#428](https://github.com/omesser/ai-buddy/issues/428) study.
- This VM took no Task Manager crop.
- Idle, walking, chat, multi-monitor, and hidden were not staged. Those rows did not launch a process.

**Tools:**

- `scripts/bench-gpu-compositing-windows.ps1`
- On Windows the GPU% order is `\GPU Engine(*)\Utilization Percentage` summed for `dwm.exe` `engtype_3D`, then WMI class `Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine`, then `nvidia-smi` for the whole adapter
- Power from `nvidia-smi` `power.draw` when that field is numeric
- `mask_rebuild:` lines as the `SetWindowRgn` call count. Per-call time stays in the [#428](https://github.com/omesser/ai-buddy/issues/428) study.
- Cursor targets multiply `frame:` point positions by the primary monitor DPI / 96. The frame loop divides the OS cursor by that scale before the engine uses it
- `dwm.exe` CPU as percent of one core. Read GPU% from the GPU% column

**Metrics (`matrix --seconds 2` on this VM):**

| Scenario | GPU% | Power W | xperf frame ms | Mask calls | Mask Hz | dwm CPU% | Notes |
|----------|------|---------|----------------|------------|---------|----------|-------|
| Baseline (no ai-buddy) | N/A | N/A | N/A | N/A | N/A | N/A | Tool sample only. No DWM |
| Idle perched | N/A | N/A | N/A | N/A | N/A | N/A | Not staged |
| Walking | N/A | N/A | N/A | N/A | N/A | N/A | Not staged |
| Chat open | N/A | N/A | N/A | N/A | N/A | N/A | Not staged |
| Multi-monitor | N/A | N/A | N/A | N/A | N/A | N/A | Not staged |
| Hidden (fullscreen) | N/A | N/A | N/A | N/A | N/A | N/A | Not staged |

The baseline note from the script was `not Windows and nvidia-smi is not on PATH`. Every other note was `not Windows. DWM compositing is not on this host`.

**Findings:**

1. **GPU%, power, and xperf frame time are unread.** The script reported that this host is not Windows and that `nvidia-smi` is not on PATH. `xperf` and `wpr` are absent. A 0 in any of those columns would be a guess.

2. **The mask columns are unread on this VM.** The clean DESKTOP table has the pointer-away rates. [#428](https://github.com/omesser/ai-buddy/issues/428) measured about 44.4 rebuilds/s when a walk stayed under the cursor. This issue's walking-over row was aborted, so that 44.4/s figure stays in #428.

3. **No Task Manager crop.** `--shot` printed `shot=N/A` and created no file.

**Evidence:**

```
os=Ubuntu 24.04.4 LTS
windows=False
nvidia_smi=absent
xperf=absent
wpr=absent
dwm_pid=N/A
xperf_frame_ms=N/A
xperf_frame_reason=xperf and wpr are not on PATH
cursor_scale=N/A
seconds=2
```

`parse-log --seconds 2` on a throwaway log with 4 `mask_rebuild:` lines printed `mask_calls=4` and `mask_hz=2.00`. That log was not produced by ai-buddy on Windows. It checks the counter.

### DESKTOP-UQIE144

Clean run on 2026-09-26, about 04:37 to 04:45 Asia/Jerusalem. Worktree `ai-buddy-430-gpu`, detached at `30647a6d593ae8a3bceb4525165782e3f05713d6`. That SHA is the readme commit on main. It does not contain `scripts/bench-gpu-compositing-windows.ps1`. The capture copied that script in from this branch and ran one session that mirrored it (trace flags, mask and frame parsing, sprite-center formula, `nvidia-smi`, and the GPU Engine counter) plus a Task Manager crop during the same Buddy lifetime. Debug `ai-buddy.exe`, BMO, sprite 126×128. `AI_BUDDY_TRACE_MASK_REBUILD=1` and `AI_BUDDY_TRACE_FRAMES=1` were set on pid 5760. Primary 3440×1440 at (0,0). Secondary 1200×1920 at (-1200,-209). `MWOClient.exe` was confirmed absent before the start and after the stop. Chrome and Grok Bot were up at low GPU%. Evidence on that machine is `.verify/430-gpu-clean/`. The idle crop there is `.verify/430-gpu-clean/idle-gpu.png` (919×824). This checkout did not contain the PNG.

**Metrics (clean, primary):**

| Scenario | Duration | Mask calls | Mask Hz | GPU as read | Notes |
|----------|----------|------------|---------|-------------|-------|
| Idle perched, cursor at (50,50) | 18.0 s | 0 | 0.00 | Task Manager GPU 0 Intel UHD 0%. Sidebar GPU 1 NVIDIA GTX 1070 9% at 49 degrees. `nvidia-smi` 12 to 17%, 13.3 to 15.1 W, 49 degrees, about 1377 to 1403 MiB. ai-buddy absent from GPU Engine. dwm 3D about 1.2 to 2.8%. chrome about 0.2 to 4.3%. Grok Bot about 1.4 to 1.6%. | `SetCursorPos(50,50)` succeeded. This window also logged 635 `walk#` frames with the pointer away. |
| Walking, pointer away | 16.3 s quiet slice | 0 | 0.00 | `nvidia-smi` 3 to 4%, 13.0 to 14.5 W, 49 degrees. ai-buddy absent. dwm 3D about 0.28 to 0.30%. Grok Bot about 1.4 to 1.5% 3D. | The 3 to 4% slice had `walk_frames=0`. It is an idle gap. The session log has 10470 `walk#` frames and 2045 `climb#` frames, and the whole log has 0 `mask_rebuild:` lines. |
| Walking-over | aborted | N/A | N/A | N/A | `SetCursorPos` to the sprite center returned false. The cursor stayed at (50,50). No walking-over rate. [#428](https://github.com/omesser/ai-buddy/issues/428) measured about 44.4 rebuilds/s with the cursor on the sprite. |
| Chat open | N/A | N/A | N/A | N/A | Not run |
| Multi-monitor sample | N/A | N/A | N/A | N/A | Both displays were covered. No separate sample. |
| Hidden | N/A | N/A | N/A | N/A | Not staged |
| xperf frame time | N/A | N/A | N/A | N/A | GPU-Z, Afterburner, and xperf were not used |

The whole `buddy.log` has 0 `mask_rebuild:` lines. The trace flag was on in the process environment. The pointer never reached the sprite, which matches the cursor-over gate. Issue #430 expected 3 to 10 rebuilds/s while walking. The pointer-away walks in this log did not rebuild. The cursor-on rate remains the #428 figure.

Buddy pid 5760 was force-killed after `CloseMainWindow` did not exit. No `ai-buddy.exe` was left. `MWOClient.exe` was still absent.

**Contaminated appendix.** An earlier pass the same morning had MechWarrior Online borderless on the primary. Do not attribute it to ai-buddy. Evidence is `.verify/430-gpu/`.

| Reading | Value | Why it is not Buddy's GPU% |
|---------|-------|----------------------------|
| `nvidia-smi` spike | 99%, about 149 W | MWOClient on the NVIDIA 3D engine, about 84 to 86% |
| Quieter `nvidia-smi` | 14%, 56.74 W | Same pass, game still running |
| NVIDIA sidebar | 6% in one crop, 6 to 93% across shots | Tracks the game, not a stable Buddy sample |
| Intel GPU 0 | 0% | Task Manager main pane |
| dwm 3D | about 0.8 to 1.9% | NVIDIA LUID, pid 2060 |
| ai-buddy GPU Engine | absent | pid 2004 |
| Mask rate | 0/s | Cursor away. The walk attempt had no confirmed walk |

**Status:** Partial. This section leaves [#430](https://github.com/omesser/ai-buddy/issues/430) open. With the game closed, idle perched is mask 0.00/s and the pointer-away log is mask 0.00/s. Walking-over was not measured. Chat, a separate multi-monitor sample, a staged hidden row, and an xperf frame time are still open. GPU-Z and Afterburner were not used. The watts in the clean table are `nvidia-smi` on the GTX 1070, and ai-buddy has no GPU Engine row.

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
