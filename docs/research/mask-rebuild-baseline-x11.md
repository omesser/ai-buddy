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

### Fast Animation (BMO react, 10 fps)

**Method:** BMO with `react` animation (2 frames at 10 fps, the fastest in BMO's manifest) triggered by poke interactions. Limited sample from startup interactions with cursor near sprite.

**Measurements (tip `6b4acbf`):**
- Sprite: BMO 126×128@1x
- Opaque pixels: **6360**
- Samples: **3 rebuilds** during react triggers
- Times: **9.611 ms, 9.354 ms, 9.724 ms**
- Average: **~9.6 ms/rebuild**

The fast animation frame rate (10 fps) doesn't increase per-rebuild cost — it remains driven by the opaque pixel count (~6360). At 10 fps with continuous cursor-over, the animation could trigger up to ~10 rebuilds/sec, but each rebuild costs the same ~10ms as other BMO frames with similar opaque counts.

### Large Sprite (Black Mage, scale=3)

**Method:** Black Mage character package (`scale = 3` in manifest), measuring startup and limited interaction. The 3× scale renders the sprite larger on screen but does not multiply the source opaque cell count.

**Measurements (tip `6b4acbf`):**
- Sprite: **37×33@3x** (source 37×33, rendered 111×99)
- Opaque pixels: **477–579** (varies by frame)
- Samples: **3 rebuilds**
- Times: **0.970 ms, 1.937 ms, 1.263 ms**
- Average: **~1.4 ms/rebuild**

Black Mage rebuilds **much faster** than BMO (~1.4ms vs ~10-15ms) because it has far fewer opaque source pixels (477–579 vs 6231–7888). Scale affects rendered size and pixmap rectangle dimensions, but rebuild cost is dominated by the iteration over opaque source cells. A 3× scale means each source cell becomes a 3×3 rectangle in the pixmap, but the loop count is the source opaque count, not the rendered pixel count.

### Fast Animation (BMO react, 10 fps)

**Status:** Measured (startup interactions, limited sample).

**Method:** BMO with `react` animation triggered by poke interactions. The `react` animation runs at 10 fps (2 frames), the fastest declared animation in BMO's manifest.

**Measurements (current tip `6b4acbf`):**
- Sprite: BMO 126×128@1x
- Opaque pixels: 6360
- Rebuild time: **~9.6–9.7 ms** (3 samples during react triggers)
- Average: **~9.6 ms**

**Findings:** The "fast" animation (10 fps vs typical 1-8 fps for other BMO animations) shows the same rebuild cost as other BMO frames with similar opaque counts. Rebuild cost is driven by opaque pixel count, not animation FPS. The 10 fps rate would produce up to 10 rebuilds/sec **if** the cursor stayed over the sprite during the full animation sequence, but each rebuild's cost remains ~10ms per the opaque count.

### Large Sprite (Black Mage at scale=3)

**Status:** Measured (startup + limited interaction).

**Method:** Black Mage character package, which declares `scale = 3` in its manifest. This renders the sprite 3× larger on screen compared to scale=1.

**Measurements (current tip `6b4acbf`):**
- Sprite: 37×33@3x scale (source sprite is 37×33 pixels, rendered at 111×99 on screen)
- Opaque pixels: **477–579** (varies by animation frame)
- Rebuild time: **0.97–1.94 ms** (3 samples)
- Average: **~1.4 ms**

**Findings:** Black Mage at scale=3 rebuilds **much faster** than BMO@1x (~1.4ms vs ~10-15ms) because it has far fewer opaque pixels (477–579 vs 6360). Scale multiplies the rendered size and the rectangle size in the 1-bit pixmap, but it does **not** multiply the source opaque cell count. A 3× scale means each source cell becomes a 3×3 rectangle in the pixmap, but the iteration count is still the source cell count. Cost is dominated by opaque count, not scale.

Black Mage is a genuinely smaller source sprite (37×33) scaled up to appear larger, not a large-source sprite. For measuring the cost of a large opaque count at higher scale, one would need a character with both large source dimensions and dense opaque regions — not present in the current shipped character set.

### Small Sprite

**Status:** Not measured. No shipped character package has a genuinely smaller sprite than the available options at scale=1. Black Mage is smaller in source dimensions but tested at scale=3. Dropped from the scenario matrix.

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

1. **Mask rebuilds are expensive:** ~10–15 ms for BMO (126×128, ~6300 opaque) at 1x on this box, ~1.4 ms for Black Mage (37×33, ~500 opaque) at 3x. Cost is dominated by opaque source pixel count, not rendered size or scale factor.

2. **Rebuilds are gated on cursor-over (and hold/control):** With the pointer elsewhere, idle and even long walk sequences log no `mask_rebuild` lines. Cost shows up when the user is interacting over the sprite.

3. **Walking under the cursor rebuilds at motion rate:** Because `MaskParams` includes position, expect tens of rebuilds/sec while walking under the pointer (measured ~27/s), not only the 8 fps walk animation rate.

4. **Scale multiplies rendered size, not opaque count:** Black Mage at scale=3 has a smaller source sprite (37×33) than BMO@1x (126×128), so despite the 3× scale it rebuilds much faster (~1.4ms vs ~10-15ms). The iteration count is the source opaque cells; scale only affects the rectangle dimensions in the pixmap.

5. **Fast animation FPS doesn't directly increase rebuild cost:** BMO `react` at 10 fps shows the same ~10ms rebuild time as other BMO animations with similar opaque counts. FPS affects how often rebuilds might fire (if cursor stays over sprite), but each rebuild's cost remains driven by the opaque pixel count.

## Evidence

### Grok Bot desktop

- `/workspace/968-box-bench/idle-15s.log` — idle measure, 0 rebuilds in window
- `/workspace/968-box-bench/idle-summary.txt`
- `/workspace/968-box-bench/walk-perch-cursor.log` — walk under cursor, 14 bout rebuilds
- `/workspace/968-box-bench/walk-summary.txt`
- `/workspace/968-box-bench/environment.txt`
- `/workspace/968-measurements/bmo-react-sustained.log` — BMO react animation samples (tip `6b4acbf`)
- `/workspace/968-measurements/blackmage-drag.log` — Black Mage scale=3 samples (tip `6b4acbf`)
- `/workspace/968-measurements/large-blackmage-cursor-over.log` — Black Mage scale=3 additional samples

### Instrumentation sample

**BMO walking under cursor:**
```
mask_rebuild: 126x128 @1x scale, 6251 opaque pixels, 13.908 ms
mask_rebuild: 126x128 @1x scale, 6298 opaque pixels, 15.328 ms
mask_rebuild: 126x128 @1x scale, 6231 opaque pixels, 12.332 ms
```

**BMO react (10 fps fast animation):**
```
mask_rebuild: 126x128 @1x scale, 6360 opaque pixels, 9.611 ms
mask_rebuild: 126x128 @1x scale, 6360 opaque pixels, 9.354 ms
mask_rebuild: 126x128 @1x scale, 6360 opaque pixels, 9.724 ms
```

**Black Mage (37×33 source @3x scale, large rendered sprite):**
```
mask_rebuild: 37x33 @3x scale, 477 opaque pixels, 0.970 ms
mask_rebuild: 37x33 @3x scale, 579 opaque pixels, 1.937 ms
mask_rebuild: 37x33 @3x scale, 579 opaque pixels, 1.263 ms
```

### Code

- `src-tauri/src/platform/x11/overlay.rs` — timing / counters
- `scripts/bench-mask-rebuild-x11.sh` — benchmark script
- `src-tauri/src/frame_loop.rs` — `MaskParams` cache + cursor-over gate

## Comparison to Issue #432 (System Wakeups)

Issue #432 measured ~271 voluntary context switches/sec during idle perched on Linux (cloud). Mask rebuilds at 0/sec with the cursor away are **not** a contributor to that idle wakeup rate. The ~15 ms rebuild time matters when the cursor is over an animating/moving sprite.

## Recommendations for Follow-Up

1. **Longer sustained animation samples** under cursor for fast animation and scale scenarios — current measurements are from startup interactions. Extended cursor-over sessions (e.g., via perch + MCP behavior triggers, or computerUse drag) would provide more stable rate and timing data.
2. **Separate counters** for "opaque set changed" vs "position-only" rebuilds if optimizing the cache key — the position-inclusive `MaskParams` drives high rebuild rates during motion even when the mask bits don't change.
3. **Windows baseline** — X11 measurements complete for shipped character scenarios. Windows DWM click-through shaping (issue #428 parent task) remains unmeasured pending DESKTOP green light.
4. **`perf` flamegraph** for rebuild hotspots — not yet collected.

