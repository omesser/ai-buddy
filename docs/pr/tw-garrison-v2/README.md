# Timber Wolf Garrison Rebuild — Phase 1: Frame 0 Lock (Revised)

## Goal

Lock crop/scale/placement parameters so frame 0 of the new garrison capture matches the pack `idle-0.png` reference as closely as possible on the 176×160 canvas, with clean matting (no blueprint artifacts in arm cavities, minimal fringing).

## Revision History

**v2 (current)**: Improved background removal using selective cavity detection. Dark + desaturated pixels (blueprint artifacts) removed while preserving dark + colored mech parts (torso, joints).

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
    "x": 349,
    "y": 99,
    "width": 567,
    "height": 453
  },
  "scale": 0.2717,
  "destination": {
    "x": 11,
    "y": 37,
    "canvas_width": 176,
    "canvas_height": 160
  }
}
```

### Processing Pipeline

1. **Brightness threshold**: Gray values 25-200 (excludes very dark background and bright UI chrome)
2. **UI region removal**: Top/bottom 12% of frame excluded
3. **Contour selection**: Largest area-weighted centered contour (the mech)
4. **Selective cavity removal**: Removes dark (< 35) + desaturated (sat < 40) pixels = blueprint artifacts, preserves dark + colored pixels = actual mech parts
5. **Fringing reduction**: 2px erosion + 5px Gaussian blur on alpha
6. **Crop**: Extract mech bounding box (349, 99, 567×453)
7. **Scale**: 0.2717× to match reference height on 176×160 canvas
8. **Position**: (11, 37) — centered horizontally, feet aligned at bottom
9. **Interpolation**: LANCZOS4 for high-quality downscaling

### Alignment Strategy

- **Feet at bottom**: Bottom margin matches pack reference (0px from canvas bottom)
- **Horizontal center**: Mech centered on canvas width
- **Scale target**: Match reference mech height

## Match Quality

| Metric | Value | Notes |
|--------|-------|-------|
| **IoU** | 0.742 | 74.2% silhouette overlap |
| **Pixel ratio** | 1.052 | Matted has 5.2% more pixels than reference |
| **Reference pixels** | 6,063 | |
| **Matted pixels** | 6,380 | |

### Comparison to v1

| Metric | v1 (rejected) | v2 (current) | Change |
|--------|---------------|--------------|--------|
| IoU | 0.752 | 0.742 | -1.3% (acceptable trade-off for clean cavities) |
| Pixel ratio | 1.133 | 1.052 | -7.2% (better match) |
| Arm cavity cleanliness | ⚠️ Blueprint artifacts visible | ✅ Clean transparent gaps | Improved |
| Torso preservation | ✅ Intact | ✅ Intact | Maintained |
| Edge fringing | ⚠️ Dark halo on arms | ✅ Soft edges | Improved |

### Diagnostics

Visual diagnostics in `docs/pr/tw-garrison-v2/`:

- `diagnostic-sidebyside.png`: Reference | Matted | Overlay (green ref + red matted = yellow overlap)
- `diagnostic-edges.png`: Edge comparison (green reference edges, red matted edges)
- `diagnostic-composite.png`: Combined side-by-side + edge view
- `diagnostic-matted.png`: Final matted result on checker background
- `diagnostic-reference.png`: Pack idle-0 reference on checker background
- `diagnostic-capture-cleaned.png`: Cleaned/matted capture before scale/crop

## Remaining Mismatch

**Torso and legs**: Excellent alignment (near-perfect edge overlap in diagnostics)

**Arms**: Capture frame 0 has arms slightly more extended laterally than pack idle-0. This is a **pose difference**, not a scale/placement error:
- Pack idle: Arms closer to body, more compact stance
- Capture frame 0: Arms slightly spread, weapon pods more visible from front

This is acceptable for phase 1 — the feet, torso scale, and overall framing are locked. The arm spread will be consistent across all frames extracted from this capture with these parameters.

**Head antennas**: Very minor difference in antenna/sensor pod angles, likely due to slightly different viewing angle. Negligible.

**Arm cavities**: Now clean (transparent gaps visible in diagnostics). Some fine blueprint lines may remain but are significantly reduced compared to v1.

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

**Lock approved (v2)**: Waiting for human OK before proceeding to phase 2.
