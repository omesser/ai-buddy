# X11 Click-Through Mask Rebuild Baseline

Benchmark for issue [#428](https://github.com/omesser/ai-buddy/issues/428): per-pixel cost of rebuilding the X11 input region (XShapeCombineMask) when the sprite animation frame changes.

## Environment

- **OS**: Ubuntu 24.04.4 LTS (Noble)
- **Kernel**: 6.12.94+ (cloud VM, not bare metal)
- **Display**: X11 via Xtigervnc (single 1920x1200 display)
- **X server**: The X.Org Foundation version 21.1.11
- **Character**: BMO (126x128 px sprite, ~6360-7888 opaque pixels depending on frame)

## Method

### Instrumentation

Added timing instrumentation to `src-tauri/src/platform/x11/overlay.rs`:
- Atomic counters for rebuild count and total nanoseconds
- Per-rebuild timing from start of `apply_input_mask()` to X11 flush completion
- Logging of sprite dimensions, scale, opaque pixel count, and rebuild time

The rebuild happens in `apply_input_mask()`:
1. Create a 1-bit pixmap at `width * scale` by `height * scale`
2. Iterate every cell of the source mask
3. Call `poly_fill_rectangle` once per opaque source cell, as a `scale` by `scale` rectangle
4. Call `shape::mask` (XShapeCombineMask)
5. Optionally union hotspot rectangles with `shape::rectangles`
6. Flush X11

Scale multiplies rectangle size and pixmap size. It does not multiply the opaque source count.

### Measurement

Benchmark script `scripts/bench-mask-rebuild-x11.sh` runs the app with `AI_BUDDY_TRACE_MASK_REBUILD=1` to log each rebuild, then parses the log for timing data.

## Results

### Idle Perched (BMO, 126x128@1x)

**10-second sample:**
- Total rebuilds: 4
- Rate: 0.4 rebuilds/sec
- Average time: **13.119 ms/rebuild**
- Range: 9.7 - 15.3 ms
- Opaque pixels: 6360-7888 (frame-dependent)

**60-second sample:**
- Total rebuilds: 3
- Rate: **0.05 rebuilds/sec**
- Average time: **11.412 ms/rebuild**
- Range: 9.5 - 13.5 ms
- Opaque pixels: 6360 (same frame repeated)

### Per-Pixel Cost

For a 126x128 sprite with ~6360-7888 opaque source cells at 1x scale:
- **~1.5-2.0 μs per opaque source cell at 1x** (calculated from 11-13 ms / 6360-7888 cells)
- This ratio is one measured size. It is not a scale law: 4x does not multiply the cell count.

### Idle Animation Rebuilds

The sprite is **not static** during "idle perched". BMO's idle animation causes occasional frame changes (every 15-20 seconds based on the 3 rebuilds in 60s), which triggers mask rebuilds. This is expected behavior: idle animations provide life.

**Comparison to expectation**: Issue stated "idle perched (expect 0 rebuilds)". Measured 0.05 rebuilds/sec, which is very low but not zero. Rebuilds happen only when the animation frame changes, not on every engine tick.

## Inconclusive Scenarios

The following scenarios require GUI interaction or specific character packages not available in the headless cloud environment:

### Walking Animation
**Status**: Inconclusive

**Why**: Walking requires user interaction (dragging the sprite or letting it walk across displays). In a headless VNC environment, automated GUI interaction was not implemented for this benchmark.

**Expected behavior**: Would see rebuilds at animation frame rate (e.g., 10-15 FPS for walking = 10-15 rebuilds/sec).

### Fast Animation
**Status**: Inconclusive

**Why**: No fast-animating character package available, and no mechanism to trigger fast animations in headless environment.

### Large Sprite (128x128@4x)
**Status**: Not measured at 4x

**Measured**: BMO at 126x128@1x with 6360-7888 opaque mask cells (see Results)
**Target**: 128x128@4x is a 512x512 pixmap

Same opaque count, larger rects / pixmap — **not measured**; do not use linear opaque extrapolation. BMO at 4x still has ~6360-7888 opaque mask cells; each becomes a `scale` by `scale` `poly_fill_rectangle`, and the pixmap grows from 126x128 to 504x512. A 128x128 mask at 4x is a 512x512 pixmap with that mask's own opaque source count, not a scaled-up cell count. Cost may grow with scale (bigger fills + bigger pixmap).

### Small Sprite (32x32@1x)
**Status**: Not measured

**Why**: No 32x32 character package available in the build.

**Expected** (not measured): a 32x32 mask at ~50% opaque is ~512 opaque source cells, which is a smaller mask, not a scale change. At the 1x ratio that sketch is ~0.8 ms/rebuild. Fixed pixmap and flush cost may dominate, so do not treat it as a measured scale law.

## Findings

1. **Mask rebuilds are expensive**: 11-13 ms for a 126x128 sprite at 1x scale is significant (about 1 frame at 60 FPS).

2. **Rebuilds only on frame change**: The code correctly caches mask parameters and rebuilds only when they change (different animation frame). Idle scenarios show very low rebuild rates (0.05/sec).

3. **Scale is not more opaque cells**: The 1x sample is ~1.5-2.0 μs per opaque source cell. `apply_input_mask()` issues one `poly_fill_rectangle` per opaque source cell; scale only enlarges that rectangle and the pixmap. Cost at 4x was **not measured**. Do not use linear opaque extrapolation.

4. **No per-tick overhead**: When the animation frame doesn't change, no rebuild happens. The mask stays applied.

## Evidence

### Instrumentation Output

```
mask_rebuild: 126x128 @1x scale, 6360 opaque pixels, 15.130 ms
mask_rebuild: 126x128 @1x scale, 6360 opaque pixels, 9.724 ms
mask_rebuild: 126x128 @1x scale, 7888 opaque pixels, 15.299 ms
mask_rebuild: 126x128 @1x scale, 7888 opaque pixels, 12.325 ms
```

### Benchmark Logs

Saved in:
- `/tmp/bench-idle-10s.log` (4 rebuilds in 10s)
- `/tmp/bench-idle-60s.log` (3 rebuilds in 60s)

### Code Changes

Instrumentation added to:
- `src-tauri/src/platform/x11/overlay.rs`: Timing and counters
- `scripts/bench-mask-rebuild-x11.sh`: Benchmark script

## Comparison to Issue #432 (System Wakeups)

Issue #432 measured ~271 voluntary context switches/sec (used as wakeup proxy) during idle perched on Linux. Mask rebuilds account for only 0.05/sec of that, so **mask rebuilds are not a significant contributor to idle wakeup rate**.

The 11-13 ms rebuild time is relevant when animation is active, not for idle power consumption.

## Recommendations for Follow-Up

1. **Bare-metal testing**: A real Linux desktop (not VM) would allow:
   - GUI automation to test walking animations
   - Real multi-monitor testing
   - Measurement of actual CPU time (not wall time affected by VM scheduling)

2. **Direct measurement of the gaps**: A 32x32 package and a real 4x run would replace the unmeasured cases. At 4x, measure the same opaque source count drawn as larger rects into a larger pixmap. Do not fill that gap with linear opaque extrapolation.

3. **Optimization scope** (out of scope for this baseline):
   - Cache opaque pixel rectangles per frame to avoid per-pixel iteration
   - Batch rectangle draws before XShapeCombineMask
   - Use XShapeCombineRegion with pre-computed regions
