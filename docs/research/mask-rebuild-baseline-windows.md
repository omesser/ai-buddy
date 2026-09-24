# Windows Click-Through Mask Rebuild Baseline

Benchmark for issue [#428](https://github.com/omesser/ai-buddy/issues/428): per-pixel cost of rebuilding the Windows DWM click-through region when the sprite animation frame changes.

## Environment

### Oded's DESKTOP-UQIE144 workstation

- **Host**: DESKTOP-UQIE144 (machineId `9723dbe7-21c6-40c0-afcd-2172f499de66`)
- **OS**: Microsoft Windows 11 Pro for Workstations version 10.0.26200 (Build 26200)
- **Display**: Primary 3440×1440 at (0,0) work area height 1392; Secondary 1200×1920 at (-1200,-209)
- **Character**: BMO (126×128 px sprite, ~6231–7888 opaque mask cells depending on frame); Black Mage (37×33 source @3x scale, ~570–630 opaque)
- **Binary**: Debug `ai-buddy.exe` at tip `a38ba3b48bb43ed4b909cb26866b6b1af97dbcd1` (PR #981 TRACE instrumentation)
- **Evidence**: `docs/research/mask-rebuild-baseline-windows/` (slim pack: environment, summaries, sample lines)

This is Oded's development workstation with a real Windows GUI. Cursor automation used `SetCursorPos` to sprite center derived from `TRACE_FRAMES` instrumentation (not human cursor movement).

## Method

### Instrumentation

Timing in Windows click-through mask rebuild path (PR #981 tip `a38ba3b` — instrumentation not necessarily merged at time of measurement):

- Atomic counters for rebuild count and total nanoseconds
- Per-rebuild timing from start to DWM region update completion
- Logging of sprite dimensions, scale, opaque pixel count, and rebuild time when `AI_BUDDY_TRACE_MASK_REBUILD=1`

The Windows DWM rebuild iterates the source mask and constructs a region from opaque cells. Scale multiplies rectangle size, not the opaque source count.

### When rebuilds run

The overlay only updates the input region while the cursor is over the sprite (or a control / during a hold). Walking under the cursor rebuilds at motion rate because the cache key includes sprite position.

### Measurement

- **Cursor method**: `SetCursorPos` to sprite center from `TRACE_FRAMES` `sprite(x,y)` lines (not human; not moving perch under fixed cursor)
- **Behavior trigger**:
  - **walk**: StaticDirector spontaneous walk (MCP `play_behavior` not used — Settings Bearer token not obtained)
  - **fast**: Rush `SetCursorPos` onto sprite (rush_reaction=react) + `mouse_event` LBUTTON poke
  - **large**: Cursor follow only
- **Environment variable**: `AI_BUDDY_TRACE_MASK_REBUILD=1` (plus `TRACE_FRAMES` for correlation)

**Method gaps documented honestly:**

1. **MCP `play_behavior` not used** — Settings Bearer token (BYO) not obtained; walk triggered via StaticDirector spontaneous behavior.
2. **Walk numbers are walk-attributed subset only** — Opaque 6231–6298 matches X11 walk frames; full 15s session included climb/react rebuilds while cursor tracked sprite, but the walk scenario isolates walk-attributed rebuilds only.
3. **Fast rate ≠ continuous 10 fps** — React animation is loop=once; per-rebuild ~22 ms is the useful metric, not sustained rate.

## Results

### Idle perched (BMO, 126×128@1x), cursor not over sprite

**12-second measure window**:

- Total rebuilds: **0**
- Rate: **0.0 rebuilds/sec**

With `SetCursorPos(50,50)` away from the sprite on primary display, idle animation produced **no** mask rebuilds, matching the cursor-over gate.

### Walking (BMO walk), cursor over sprite

**Method:** `SetCursorPos` follow `TRACE_FRAMES` sprite center; StaticDirector spontaneous walk. Rebuild lines attributed to walk animation.

**Walk-attributed subset from ~5.0-second bout:**

- Total rebuilds: **223**
- Rate: **~44.4 rebuilds/sec** (motion-driven; cache key includes `x,y`)
- Average time: **14.54 ms/rebuild**
- Range: 14.03 – 15.74 ms
- Opaque pixels: **6231–6298** (matches X11 walk frames)

The walk-only subset isolates rebuilds attributed to walk animation. The full ~15s cursor-follow session also included climb (351 rebuilds) and react (40 rebuilds); walk scenario reports only the walk-attributed 223 rebuilds over ~5.0 seconds of walk.

Opaque counts 6231–6298 match the X11 walk baseline, confirming the same sprite frames.

### Fast Animation (BMO react, 10 fps)

**Method:** Rush `SetCursorPos` onto sprite (BMO rush_reaction=react) + LBUTTON poke; hold cursor over during react frames.

**React-only rebuilds over 12.1 seconds:**

- Total rebuilds: **68**
- Rate: **5.6 rebuilds/sec** (react is loop=once, not sustained 10 fps)
- Average time: **21.79 ms/rebuild**
- Range: 17.3 – 30.82 ms
- Opaque pixels: **6360–7888**

Fast animation frame rate (10 fps) doesn't increase per-rebuild cost — each rebuild is driven by opaque pixel count. The ~22 ms per-rebuild time is the useful metric; react loop=once means the rate reflects trigger frequency, not continuous animation.

A supplemental sample from the walk session (40 rebuilds when rush onto walking sprite triggered react under cursor) measured avg 23.6 ms, min 17.11 ms, max 30.03 ms — consistent with the primary fast measurement.

### Large Sprite (Black Mage, scale=3)

**Method:** `AI_BUDDY_INSTANCES='Black Mage'` (scale=3 in manifest); `SetCursorPos` follow sprite center.

**Measurements over 12.1 seconds:**

- Sprite: **111×99 rendered (@3x scale from 37×33 source)**
- Total rebuilds: **6**
- Rate: **0.5 rebuilds/sec**
- Average time: **2.50 ms/rebuild**
- Range: 2.03 – 2.77 ms
- Opaque pixels: **570–630**

Black Mage rebuilds **much faster** than BMO (~2.5ms vs ~15-22ms) because it has far fewer opaque source pixels (570–630 vs 6231–7888). Scale affects rendered size and rectangle dimensions, but rebuild cost is dominated by the iteration over opaque source cells.

By animation: idle 4 rebuilds (avg 2.55 ms, opaque 570–579); react 2 rebuilds (avg 2.40 ms, opaque 579–630).

### Small Sprite

**Status:** Not measured. No shipped character package has a genuinely smaller sprite than the available options at scale=1. Black Mage is smaller in source dimensions but tested at scale=3. Dropped from the scenario matrix.

## Comparison to X11 Baseline

X11 baseline: `docs/research/mask-rebuild-baseline-x11.md` (issue #428, shipped in PR #968).

| Metric | Windows (DESKTOP-UQIE144) | X11 (Grok Bot desktop) |
|--------|---------------------------|------------------------|
| **Idle** (cursor away) | 0 rebuilds / 12s = **0.0/s** | 0 rebuilds / 15s = **0.0/s** |
| **Walk** (BMO, cursor over) | 223 / ~5.0s = **~44/s**, avg **14.5 ms** | 14 / ~0.52s = **~27/s**, avg **15.0 ms** |
| **Fast** (BMO react) | 68 / 12.1s = **5.6/s**, avg **21.8 ms** | 3 samples, avg **~9.6 ms** |
| **Large** (Black Mage @3x) | 6 / 12.1s = **0.5/s**, avg **2.5 ms** | 3 samples, avg **~1.4 ms** |
| **Opaque** (BMO walk) | 6231–6298 | 6231–6298 (identical) |
| **Opaque** (Black Mage) | 570–630 | 477–579 |

**Key observations:**

1. **Idle cursor-away is 0 rebuilds/sec on both** — The cursor-over gate is effective; idle animation does not trigger rebuilds when the pointer is elsewhere.
2. **Walk per-rebuild time is similar** — Windows ~14.5 ms vs X11 ~15.0 ms for the same opaque counts (6231–6298). Walk rate difference (~44/s vs ~27/s) reflects measurement session mechanics (Windows cursor-follow tracked longer or more granular motion), not per-rebuild cost.
3. **Fast (react) is slower on Windows** — Windows ~22 ms vs X11 ~10 ms. This may reflect DWM region update cost vs XShapeCombineMask, or the Windows sample included more 7888-opaque frames (react's mouth-open sprite).
4. **Large sprite (Black Mage) is cheap on both** — Windows ~2.5 ms, X11 ~1.4 ms. Both are much faster than BMO because Black Mage has far fewer opaque pixels (~570–630 vs ~6231–7888).
5. **Cost is driven by opaque pixel count, not scale** — Black Mage at 3× scale is faster than BMO at 1× because the source opaque count is lower.

## Findings

1. **Mask rebuilds are expensive on Windows:** ~15 ms for BMO walk (126×128, ~6300 opaque) at 1×; ~22 ms for BMO react (mixed 6360 and 7888 opaque); ~2.5 ms for Black Mage (37×33 source, ~570–630 opaque) at 3×. Cost is dominated by opaque source pixel count, not rendered size or scale factor.

2. **Rebuilds are gated on cursor-over:** With the pointer away, idle animation logs no rebuilds. Cost shows up when the user moves the cursor over the sprite.

3. **Walking under the cursor rebuilds at motion rate:** Because the cache key includes position, walking under the pointer triggers rebuilds for every position change, not only when the animation frame's opaque set changes. Windows walk measured ~44 rebuilds/sec during the ~5.0s walk bout under cursor-follow.

4. **Scale multiplies rendered size, not opaque count:** Black Mage at scale=3 has smaller source dimensions (37×33) than BMO@1× (126×128), so despite the 3× scale it rebuilds much faster (~2.5ms vs ~15-22ms). The iteration count is the source opaque cells; scale only affects the rectangle dimensions in the region.

5. **Windows DWM rebuild is similar to X11 XShapeCombineMask for walk, slower for react:** Walk per-rebuild times are nearly identical (~14.5 ms Windows vs ~15 ms X11 for 6231–6298 opaque). React is slower on Windows (~22 ms vs ~10 ms); this may reflect DWM region API cost or measurement differences (Windows sample included more high-opaque frames).

## Evidence

Slim evidence pack in [`docs/research/mask-rebuild-baseline-windows/`](./mask-rebuild-baseline-windows/):

- [`environment.txt`](./mask-rebuild-baseline-windows/environment.txt) — host/OS/display/binary config
- [`idle-summary.txt`](./mask-rebuild-baseline-windows/idle-summary.txt) — idle measure summary (0 rebuilds in 12s)
- [`walk-summary.txt`](./mask-rebuild-baseline-windows/walk-summary.txt) — walk-attributed subset summary (223 rebuilds)
- [`fast-summary.txt`](./mask-rebuild-baseline-windows/fast-summary.txt) — BMO react summary (68 rebuilds)
- [`large-summary.txt`](./mask-rebuild-baseline-windows/large-summary.txt) — Black Mage @3x summary (6 rebuilds)
- [`walk-mask-rebuild-sample.log`](./mask-rebuild-baseline-windows/walk-mask-rebuild-sample.log) — sample `mask_rebuild:` log lines
- [`fast-mask-rebuild-sample.log`](./mask-rebuild-baseline-windows/fast-mask-rebuild-sample.log) — fast/react sample lines
- [`large-mask-rebuild-sample.log`](./mask-rebuild-baseline-windows/large-mask-rebuild-sample.log) — Black Mage sample lines
- [`README.md`](./mask-rebuild-baseline-windows/README.md) — pack provenance

Full TRACE logs remain on the measurement workstation.

### Code

- Windows: `src-tauri/src/platform/windows/overlay.rs` — timing / counters / SetWindowRgn
- Instrumentation: PR #981 tip `a38ba3b` — TRACE instrumentation
- `src-tauri/src/frame_loop.rs` — `MaskParams` cache + cursor-over gate

## Comparison to Issue #432 (System Wakeups)

Issue #432 measured system wakeups during idle on Linux. Windows idle with cursor away shows **0 rebuilds/sec**, so mask rebuilds are **not** a contributor to idle wakeup rate on Windows either. The ~15–22 ms rebuild time matters when the cursor is over an animating/moving sprite.

### Instrumentation sample

**BMO walking under cursor:**
```
mask_rebuild: 126x128 @1x scale, 6251 opaque pixels, 14.43 ms
mask_rebuild: 126x128 @1x scale, 6251 opaque pixels, 14.51 ms
mask_rebuild: 126x128 @1x scale, 6251 opaque pixels, 14.56 ms
```

**BMO react (10 fps fast animation):**
```
mask_rebuild: 126x128 @1x scale, 6360 opaque pixels, 20.71 ms
mask_rebuild: 126x128 @1x scale, 7888 opaque pixels, 30.82 ms
mask_rebuild: 126x128 @1x scale, 6360 opaque pixels, 22.00 ms
```

**Black Mage (37×33 source @3x scale, rendered 111×99):**
```
mask_rebuild: 111x99 @3x scale, 579 opaque pixels, 2.03 ms
mask_rebuild: 111x99 @3x scale, 630 opaque pixels, 2.77 ms
mask_rebuild: 111x99 @3x scale, 579 opaque pixels, 2.64 ms
```

## Recommendations for Follow-Up

1. **`perf` or Windows Performance Analyzer flamegraph** for rebuild hotspots — not yet collected for Windows.
2. **Sustained fast animation samples** — Current react measurements are from poke interactions (loop=once). Extended cursor-over with a hypothetical looping 10 fps animation would clarify sustained rate vs per-rebuild cost.
3. **Isolate DWM region update cost vs iteration cost** — Instrument the region construction separately from the DWM API call to understand the cost breakdown.
4. **Compare debug vs release builds** — Current Windows measurements used debug `ai-buddy.exe`; release build may show different timings.
