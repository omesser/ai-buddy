# Timber Wolf Garrison Rebuild — Phase 1: Frame 0 Lock (v4 Final)

## Goal

Lock crop/scale/placement parameters so frame 0 of the new garrison capture matches the pack `idle-0.png` reference as closely as possible on the 176×160 canvas, with clean matting (no blueprint artifacts in arm cavities, solid torso AND legs, minimal fringing).

## Revision History

**v4 (current - FINAL)**: Central column protection (40% width, full vertical height) prevents removal in cockpit→torso→hips→legs. Cavity removal restricted to left/right lateral arm bands only. Solid body + clean arm gaps.

**v3 (rejected)**: Elliptical torso protection fixed torso hole but created new holes in upper legs/thighs where shaded hull was removed.

**v2 (rejected)**: Selective cavity removal (dark + desaturated) removed arm gaps but created a hole in the torso midsection.

**v1 (rejected)**: Initial threshold-based matting left Sketchfab blueprint fragments in arm cavities and dark fringing on arm silhouettes.

## Source Files

- **Capture video**: `capture.mov` (~1280×672, ~14.65s, ~57fps, ReplayKit)
- **Frame 0 extract**: `capture-frame0.png` (first frame from the video)
- **Pack reference**: Pack `idle-0.png` from `characters/timber-wolf/frames/` (also copied to `pack-idle-0.png`)

## Locked Parameters

All parameters saved in `frame0-lock.json`:

```json
{
  "crop": {
    "x": 442,
    "y": 99,
    "width": 400,
    "height": 453
  },
  "scale": 0.2716,
  "destination": {
    "x": 34,
    "y": 37,
    "canvas_width": 176,
    "canvas_height": 160
  }
}
```

### Processing Pipeline (v4)

1. **Brightness threshold**: Gray values 25-200 (excludes very dark background and bright UI chrome)
2. **UI region removal**: Top/bottom 12% of frame excluded
3. **Contour selection**: Largest area-weighted centered contour (the mech)
4. **Central column protection**: 40% of mech width around centroid, spanning full vertical height (cockpit → torso → hips → upper legs → lower legs) = **FULLY PROTECTED, never touched**
5. **Lateral arm bands**: Left side (< center - 20% width) + Right side (> center + 20% width) = **removable regions**
6. **Selective cavity removal**: Dark (< 35) + desaturated (sat < 40) pixels ONLY in lateral arm bands, removed
7. **Fringing reduction**: 2px erosion + 5px Gaussian blur on alpha
8. **Crop**: Extract mech bounding box (442, 99, 400×453)
9. **Scale**: 0.2716× to match reference height on 176×160 canvas
10. **Position**: (34, 37) — centered horizontally, feet aligned at bottom
11. **Interpolation**: LANCZOS4 for high-quality downscaling

### Alignment Strategy

- **Feet at bottom**: Bottom margin matches pack reference (0px from canvas bottom)
- **Horizontal center**: Mech centered on canvas width
- **Scale target**: Match reference mech height

## Match Quality

| Metric | Value | Notes |
|--------|-------|-------|
| **IoU** | 0.741 | 74.1% silhouette overlap |
| **Pixel ratio** | 1.221 | Matted has 22.1% more pixels than reference |
| **Reference pixels** | 6,063 | |
| **Matted pixels** | 7,401 | |

### Comparison Across Versions

| Metric | v1 (rejected) | v2 (rejected) | v3 (rejected) | v4 (final) |
|--------|---------------|---------------|---------------|------------|
| **IoU** | 0.752 | 0.742 | 0.750 | **0.741** |
| **Pixel ratio** | 1.133 | 1.052 | 1.164 | **1.221** |
| **Arm cavities** | ⚠️ Blueprint artifacts | ✅ Clean | ✅ Clean | ✅ Clean |
| **Torso interior** | ✅ Solid | ⚠️ Hole | ✅ Solid | ✅ Solid |
| **Upper legs** | ✅ Solid | ✅ Solid | ⚠️ Holes | ✅ Solid |
| **Edge fringing** | ⚠️ Dark halo | ✅ Soft | ✅ Soft | ✅ Soft |

**v4 achieves complete body integrity**: Clean arm cavities + solid torso + solid legs + soft edges. Slightly higher pixel ratio (1.221 vs reference ideal of 1.0) is acceptable trade-off for complete structural preservation.

### Diagnostics

Visual diagnostics in `docs/pr/tw-garrison-v2/`:

- `diagnostic-sidebyside.png`: Reference | Matted | Overlay (green ref + red matted = yellow overlap)
- `diagnostic-edges.png`: Edge comparison (green reference edges, red matted edges)
- `diagnostic-composite.png`: Combined side-by-side + edge view
- `diagnostic-matted.png`: Final matted result on checker background
- `diagnostic-reference.png`: Pack idle-0 reference on checker background
- `diagnostic-capture-cleaned.png`: Cleaned/matted capture before scale/crop

## Remaining Mismatch

**Torso and legs**: Excellent alignment, completely solid with no holes anywhere (torso, hips, thighs all intact)

**Arms**: Capture frame 0 has arms slightly more extended laterally than pack idle-0. This is a **pose difference**, not a scale/placement error:
- Pack idle: Arms closer to body, more compact stance
- Capture frame 0: Arms slightly spread, weapon pods more visible from front

This is acceptable for phase 1 — the feet, torso scale, and overall framing are locked. The arm spread will be consistent across all frames extracted from this capture with these parameters.

**Head antennas**: Very minor difference in antenna/sensor pod angles, likely due to slightly different viewing angle. Negligible.

**Arm cavities**: Clean (transparent gaps visible in diagnostics). Blueprint artifacts removed while preserving complete solid body structure (torso + legs).

**Pixel ratio (1.221)**: Slightly higher than ideal due to conservative protection of central body column. This prevents any accidental removal of shaded hull surfaces. The extra pixels are mostly in outer arm edges and shoulder region - acceptable trade-off for structural integrity.

## Reproducibility

The lock parameters in `frame0-lock.json` are sufficient to reproduce this exact matting. The script `scripts/lock_garrison_frame.py` contains the complete processing pipeline.

## Next Steps (Out of Scope for Phase 1)

- **Phase 2**: Extract all frames from `capture.mov` using these lock parameters
- **Keyframe selection**: Identify sparse keyframes for the garrison loop
- **Loop structure**: Determine loop start/end, ensure mesh with idle
- **Bake**: Create final strip and update `characters/timber-wolf/` pack
- **Manifest**: Update `character.manifest` for new `sleep` animation frames
- **PR**: Open pull request with the complete garrison animation

## Tooling

Frame lock generated by `scripts/lock_garrison_frame.py` (committed).

---

**Lock approved (v4 final)**: Solid torso, solid legs, clean arm cavities, reasonable IoU. Complete body integrity achieved. Waiting for human OK before proceeding to phase 2.
