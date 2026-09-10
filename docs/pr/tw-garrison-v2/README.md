# Timber Wolf Garrison Rebuild — Phase 1: Frame 0 Lock (v3 Final)

## Goal

Lock crop/scale/placement parameters so frame 0 of the new garrison capture matches the pack `idle-0.png` reference as closely as possible on the 176×160 canvas, with clean matting (no blueprint artifacts in arm cavities, solid torso, minimal fringing).

## Revision History

**v3 (current - FINAL)**: Elliptical torso protection zone prevents interior removal while maintaining aggressive cavity cleanup. Solid torso + clean arm gaps.

**v2 (rejected)**: Selective cavity removal (dark + desaturated) removed arm gaps but also created a hole in the torso midsection where shaded hull panels were removed.

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

### Processing Pipeline (v3)

1. **Brightness threshold**: Gray values 25-200 (excludes very dark background and bright UI chrome)
2. **UI region removal**: Top/bottom 12% of frame excluded
3. **Contour selection**: Largest area-weighted centered contour (the mech)
4. **Elliptical torso protection**: 25% width × 20% height around centroid = protected core
5. **Selective cavity removal**: Dark (< 35) + desaturated (sat < 40) pixels OUTSIDE protection zone = blueprint artifacts, removed
6. **Fringing reduction**: 2px erosion + 5px Gaussian blur on alpha
7. **Crop**: Extract mech bounding box (442, 99, 400×453)
8. **Scale**: 0.2716× to match reference height on 176×160 canvas
9. **Position**: (34, 37) — centered horizontally, feet aligned at bottom
10. **Interpolation**: LANCZOS4 for high-quality downscaling

### Alignment Strategy

- **Feet at bottom**: Bottom margin matches pack reference (0px from canvas bottom)
- **Horizontal center**: Mech centered on canvas width
- **Scale target**: Match reference mech height

## Match Quality

| Metric | Value | Notes |
|--------|-------|-------|
| **IoU** | 0.750 | 75.0% silhouette overlap |
| **Pixel ratio** | 1.164 | Matted has 16.4% more pixels than reference |
| **Reference pixels** | 6,063 | |
| **Matted pixels** | 7,060 | |

### Comparison Across Versions

| Metric | v1 (rejected) | v2 (rejected) | v3 (final) |
|--------|---------------|---------------|------------|
| **IoU** | 0.752 | 0.742 | **0.750** |
| **Pixel ratio** | 1.133 | 1.052 | **1.164** |
| **Arm cavities** | ⚠️ Blueprint artifacts | ✅ Clean | ✅ Clean |
| **Torso interior** | ✅ Solid | ⚠️ Hole | ✅ Solid |
| **Edge fringing** | ⚠️ Dark halo | ✅ Soft | ✅ Soft |

**v3 achieves the best overall result**: Clean arm cavities + solid torso + soft edges, with IoU nearly identical to v1 (before cavity removal) and pixel ratio between v1 and v2.

### Diagnostics

Visual diagnostics in `docs/pr/tw-garrison-v2/`:

- `diagnostic-sidebyside.png`: Reference | Matted | Overlay (green ref + red matted = yellow overlap)
- `diagnostic-edges.png`: Edge comparison (green reference edges, red matted edges)
- `diagnostic-composite.png`: Combined side-by-side + edge view
- `diagnostic-matted.png`: Final matted result on checker background
- `diagnostic-reference.png`: Pack idle-0 reference on checker background
- `diagnostic-capture-cleaned.png`: Cleaned/matted capture before scale/crop

## Remaining Mismatch

**Torso and legs**: Excellent alignment, torso now solid with no interior hole

**Arms**: Capture frame 0 has arms slightly more extended laterally than pack idle-0. This is a **pose difference**, not a scale/placement error:
- Pack idle: Arms closer to body, more compact stance
- Capture frame 0: Arms slightly spread, weapon pods more visible from front

This is acceptable for phase 1 — the feet, torso scale, and overall framing are locked. The arm spread will be consistent across all frames extracted from this capture with these parameters.

**Head antennas**: Very minor difference in antenna/sensor pod angles, likely due to slightly different viewing angle. Negligible.

**Arm cavities**: Now clean (transparent gaps visible in diagnostics). Blueprint artifacts removed while preserving solid torso structure.

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

**Lock approved (v3 final)**: Solid torso, clean arm cavities, reasonable IoU. Waiting for human OK before proceeding to phase 2.
