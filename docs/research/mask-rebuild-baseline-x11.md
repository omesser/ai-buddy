# X11 Click-Through Mask Rebuild Baseline

Benchmark for issue [#428](https://github.com/omesser/ai-buddy/issues/428): per-pixel cost of rebuilding the X11 input region (XShapeCombineMask) when the sprite animation frame changes.

## Environment

### Grok Bot Linux desktop (GUI scenarios + remeasured idle)

- **Host**: Grok Bot box (`grok-bot-vm-429178738`)
- **OS**: Debian GNU/Linux 13 (trixie)
- **Kernel**: 6.12.94+
- **Display**: X11 on `DISPLAY=:5`, **1280×800**, depth 24
- **X server**: The X.Org Foundation version 21.1.16
- **Character**: BMO (126×128 px sprite, ~6231–7888 opaque mask cells depending on frame)
- **Binary**: `cargo build -p ai-buddy` debug at tip `965e147`
- **Evidence**: `/workspace/968-box-bench/` (`idle-15s.log`, `walk-perch-cursor.log`, summaries)

This is a real agent desktop with an X11 GUI (not a headless cloud run without pointer/window automation). Walking used a mapped `xmessage` perch and the app's loopback MCP `play_behavior`, with the **stationary** pointer left over the sprite. Shell `xdotool` was not used to drive clicks/drags.

### Prior cloud-VM idle (kept for comparison)

Earlier idle numbers on a headless cloud VM (Ubuntu 24.04.4, Xtigervnc 1920×1200) remain below under **Prior cloud-VM idle**. Prefer the Grok Bot desktop numbers for GUI scenarios; cloud idle is labeled when cited.

## Method

### Instrumentation

Timing in `src-tauri/src/platform/x11/overlay.rs`:

- Atomic counters for rebuild count and total nanoseconds
- Per-rebuild timing from start of `apply_input_mask()` to X11 flush completion
- Logging of sprite dimensions, scale, opaque pixel count, and rebuild time when `AI_BUDDY_TRACE_MASK_REBUILD=1`

The rebuild happens in `apply_input_mask()`:

1. Create a 1-bit pixmap at `width * scale` by `height * scale`
2. Iterate every cell of the source mask
3. Call `poly_fill_rectangle` once per opaque source cell, as a `scale` by `scale` rectangle
4. Call `shape::mask` (XShapeCombineMask)
5. Optionally union hotspot rectangles with `shape::rectangles`
6. Flush X11

Scale multiplies rectangle size and pixmap size. It does not multiply the opaque source count.

### When rebuilds run

`frame_loop` only calls `update_input_region` while the overlay is not click-through-ignored — i.e. while the cursor is over the sprite (or a control / during a hold). `MaskParams` includes mask bits **and** sprite `x,y`, so walking under the cursor rebuilds at **motion rate**, not only when the animation frame's opaque set changes.

### Measurement

- Script: `scripts/bench-mask-rebuild-x11.sh` (idle / walk scenarios; walk still needs an interaction source)
- Box runs: app with `AI_BUDDY_TRACE_MASK_REBUILD=1` (plus `TRACE_ENGINE` / `TRACE_FRAMES` for walk correlation)

## Results — Grok Bot desktop

### Idle perched (BMO, 126×128@1x), cursor not over sprite

**15-second measure window** (startup fall excluded):

- Total rebuilds: **0**
- Rate: **0.0 rebuilds/sec**

Startup fall briefly crossed the stationary pointer and logged 3 rebuilds (~16–18 ms, 7888 opaque) before the sprite perched on the floor away from the cursor. After that, idle animation continued with **no** mask rebuilds, matching the cursor-over gate.

### Walking (BMO walk/patrol), cursor over sprite on a perch

**Method:** Map a wide `xmessage` window (geometry ~900×80 at +190+480) so BMO perches with sprite rect covering the stationary pointer at (640,400). Trigger `walk` / `patrol` via loopback MCP `play_behavior` (and StaticDirector). Correlate `mask_rebuild:` lines with `TRACE_FRAMES` while `walk#*` and cursor-over.

**First perched walk bout under the cursor (~0.52 s of walk+over frames):**

- Total rebuilds: **14**
- Rate: **~26.7 rebuilds/sec** (motion-driven; `MaskParams` includes `x,y`)
- Average time: **15.002 ms/rebuild**
- Range: 12.332 – 18.646 ms
- Opaque pixels: **6231–6298** (matches walk-2 / walk-0 / walk-1 alpha masks)

Opaque set changed across the three walk frames; many rebuilds repeated the same opaque count while `x` advanced — expected with the position-inclusive cache key.

After the sprite walked off the perch (floor at y≈615), hundreds of further `walk#*` frames produced **no** rebuilds until/unless the cursor overlapped again.

### Fast Animation

**Status:** Still not measured.

**Why:** No fast-animating character package in-tree beyond BMO's existing clips; not blocked only by GUI — package availability.

### Large Sprite (128×128@4x) / Small Sprite (32×32@1x)

Unchanged: not measured at those sizes/packages. See prior notes — do not use linear opaque extrapolation for scale.

## Prior cloud-VM idle

*(Headless cloud VM; Ubuntu 24.04.4; Xtigervnc 1920×1200; BMO.)*

### Idle Perched — 10-second sample

- Total rebuilds: 4
- Rate: 0.4 rebuilds/sec
- Average time: **13.119 ms/rebuild**
- Range: 9.7 – 15.3 ms
- Opaque pixels: 6360–7888

### Idle Perched — 60-second sample

- Total rebuilds: 3
- Rate: **0.05 rebuilds/sec**
- Average time: **11.412 ms/rebuild**
- Range: 9.5 – 13.5 ms
- Opaque pixels: 6360

Those cloud samples likely had occasional cursor-over or idle frame transitions that passed the gate; box idle with cursor away from the sprite measured **0**/sec.

### Per-Pixel Cost (from cloud 1x samples)

For ~6360–7888 opaque source cells at 1x:

- **~1.5–2.0 μs per opaque source cell at 1x**
- Not a scale law: 4x does not multiply the cell count.

## Findings

1. **Mask rebuilds are expensive:** ~12–18 ms for BMO at 1x on this box (~15 ms avg while walking under the cursor).

2. **Rebuilds are gated on cursor-over (and hold/control):** With the pointer elsewhere, idle and even long walk sequences log no `mask_rebuild` lines. Cost shows up when the user is interacting over the sprite.

3. **Walking under the cursor rebuilds at motion rate:** Because `MaskParams` includes position, expect tens of rebuilds/sec while walking under the pointer (measured ~27/s), not only the 8 fps walk animation rate.

4. **Scale is not more opaque cells:** Same model as before; 4x still unmeasured.

## Evidence

### Grok Bot desktop

- `/workspace/968-box-bench/idle-15s.log` — idle measure, 0 rebuilds in window
- `/workspace/968-box-bench/idle-summary.txt`
- `/workspace/968-box-bench/walk-perch-cursor.log` — walk under cursor, 14 bout rebuilds
- `/workspace/968-box-bench/walk-summary.txt`
- `/workspace/968-box-bench/environment.txt`

### Instrumentation sample (walking under cursor)

```
mask_rebuild: 126x128 @1x scale, 6251 opaque pixels, 13.908 ms
mask_rebuild: 126x128 @1x scale, 6298 opaque pixels, 15.328 ms
mask_rebuild: 126x128 @1x scale, 6231 opaque pixels, 12.332 ms
```

### Code

- `src-tauri/src/platform/x11/overlay.rs` — timing / counters
- `scripts/bench-mask-rebuild-x11.sh` — benchmark script
- `src-tauri/src/frame_loop.rs` — `MaskParams` cache + cursor-over gate

## Comparison to Issue #432 (System Wakeups)

Issue #432 measured ~271 voluntary context switches/sec during idle perched on Linux (cloud). Mask rebuilds at 0/sec with the cursor away are **not** a contributor to that idle wakeup rate. The ~15 ms rebuild time matters when the cursor is over an animating/moving sprite.

## Recommendations for Follow-Up

1. **Longer walk-under-cursor samples** (keep pointer over the sprite for a full 10–20 s) for a stabler rate — e.g. a shorter perch or computerUse drag.
2. **Separate counters** for “opaque set changed” vs “position-only” rebuilds if optimizing the cache key.
3. **32×32 package and real 4x run** — still unmeasured; no linear opaque extrapolation.
4. **Fast-animation package** when one exists.
