#!/usr/bin/env python3
"""
Lock crop/scale/placement parameters for Timber Wolf garrison frame 0 to match pack idle-0.

This script:
1. Loads the capture frame 0 and pack idle-0 reference
2. Removes background/UI from capture
3. Iteratively tunes crop, scale, and position to match the reference
4. Saves lock parameters as JSON
5. Generates visual diagnostics
"""

import cv2
import numpy as np
import json
from pathlib import Path

# Paths
CAPTURE_FRAME = Path("/workspace/docs/pr/tw-garrison-v2/capture-frame0.png")
PACK_IDLE = Path("/workspace/characters/timber-wolf/frames/idle-0.png")
OUTPUT_DIR = Path("/workspace/docs/pr/tw-garrison-v2")
LOCK_JSON = OUTPUT_DIR / "frame0-lock.json"

# Pack canvas size (confirmed)
CANVAS_WIDTH = 176
CANVAS_HEIGHT = 160


def remove_background(img):
    """
    Remove background from capture frame using multi-stage filtering.
    Removes Sketchfab blueprint artifacts while preserving solid torso.
    """
    if img.shape[2] == 4:
        rgb = img[:, :, :3]
    else:
        rgb = img
    
    h, w = rgb.shape[:2]
    
    # Convert to grayscale
    gray = cv2.cvtColor(rgb, cv2.COLOR_BGR2GRAY)
    hsv = cv2.cvtColor(rgb, cv2.COLOR_BGR2HSV)
    
    # Step 1: Stricter initial threshold to avoid picking up too much dark background
    # The mech has visible detail/texture even in shaded areas
    mask_bright_enough = gray > 30  # Slightly stricter than before
    
    # Also require some texture/variation (not pure uniform dark)
    # or some color saturation (colored parts of mech)
    has_color = hsv[:, :, 1] > 20
    has_texture = gray > 25  # Slightly looser for textured dark areas
    
    mask = mask_bright_enough | (has_texture & has_color)
    
    # Step 2: Remove UI regions (top and bottom 12%)
    ui_region = np.ones_like(mask, dtype=bool)
    ui_region[int(h*0.12):int(h*0.88), :] = False
    mask[ui_region] = False
    
    mask_uint = mask.astype(np.uint8) * 255
    
    # Step 3: Morphological cleanup
    kernel_small = np.ones((5, 5), np.uint8)
    mask_uint = cv2.morphologyEx(mask_uint, cv2.MORPH_CLOSE, kernel_small, iterations=2)
    
    # Step 4: Find main contour
    contours, _ = cv2.findContours(mask_uint, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    
    if contours and len(contours) > 0:
        img_center_x = w / 2
        img_center_y = h / 2
        
        def score_contour(contour):
            area = cv2.contourArea(contour)
            if area < 1000:
                return -1
            M = cv2.moments(contour)
            if M["m00"] == 0:
                return -1
            cx = M["m10"] / M["m00"]
            cy = M["m01"] / M["m00"]
            dist = np.sqrt((cx - img_center_x)**2 + (cy - img_center_y)**2)
            return area * 1000 / (dist + 100)
        
        scored = [(score_contour(c), c) for c in contours]
        scored = [(s, c) for s, c in scored if s > 0]
        
        if scored:
            best_contour = max(scored, key=lambda x: x[0])[1]
            mask_uint = np.zeros((h, w), dtype=np.uint8)
            cv2.drawContours(mask_uint, [best_contour], -1, 255, -1)
    
    # Step 5: Clean arm cavities ONLY in lateral regions
    # Protect entire central body column (cockpit → torso → hips → legs)
    
    hsv = cv2.cvtColor(rgb, cv2.COLOR_BGR2HSV)
    
    # Find mech center and bounds
    M = cv2.moments(mask_uint)
    if M["m00"] > 0:
        cx = int(M["m10"] / M["m00"])
        cy = int(M["m01"] / M["m00"])
    else:
        cx, cy = w // 2, h // 2
    
    mech_pixels = mask_uint > 0
    if np.any(mech_pixels):
        mech_y, mech_x = np.where(mech_pixels)
        mech_x_min, mech_x_max = mech_x.min(), mech_x.max()
        mech_y_min, mech_y_max = mech_y.min(), mech_y.max()
        mech_w = mech_x_max - mech_x_min
        mech_h = mech_y_max - mech_y_min
        
        # Define central body column (vertical strip) - FULLY PROTECTED
        # This includes cockpit, torso, hips, and legs
        # Use 40% of mech width as the central column
        y_coords, x_coords = np.ogrid[:h, :w]
        x_from_center = np.abs(x_coords - cx)
        central_column = x_from_center < (mech_w * 0.40)
        
        # Arm lateral bands: left and right sides ONLY
        # These are where blueprint artifacts appear (between arm and body)
        left_arm_band = (x_coords < cx - mech_w * 0.20) & (x_coords > mech_x_min)
        right_arm_band = (x_coords > cx + mech_w * 0.20) & (x_coords < mech_x_max)
        arm_regions = left_arm_band | right_arm_band
        
        # Only allow cavity removal in arm regions, NEVER in central column
        removable_region = arm_regions & (~central_column) & (mask_uint > 0)
    else:
        removable_region = np.zeros((h, w), dtype=bool)
    
    # Identify blueprint artifacts: dark + desaturated
    internal_dark = (gray < 35) & removable_region
    internal_gray = (hsv[:, :, 1] < 40) & removable_region
    
    # Candidate artifacts (ONLY in removable arm regions)
    blueprint_artifacts = internal_dark & internal_gray
    
    # Morphological cleanup: keep coherent cavity regions
    kernel_cavity = np.ones((5, 5), np.uint8)
    dark_cavities = cv2.morphologyEx(blueprint_artifacts.astype(np.uint8) * 255, 
                                      cv2.MORPH_OPEN, kernel_cavity, iterations=1)
    
    # Remove cavities (only in arm bands, central column untouched)
    mask_uint[dark_cavities > 0] = 0
    
    # Step 6: Reduce fringing
    kernel_erode = np.ones((2, 2), np.uint8)
    mask_uint = cv2.erode(mask_uint, kernel_erode, iterations=1)
    
    # Step 7: Smooth alpha edges
    mask_float = mask_uint.astype(np.float32) / 255.0
    mask_smooth = cv2.GaussianBlur(mask_float, (5, 5), 1.0)
    mask_final = (mask_smooth * 255).astype(np.uint8)
    
    # Step 8: Create RGBA output
    result = np.zeros((h, w, 4), dtype=np.uint8)
    result[:, :, :3] = rgb
    result[:, :, 3] = mask_final
    
    return result, mask_final


def find_content_bounds(mask):
    """Find the bounding box of non-zero pixels in the mask."""
    coords = cv2.findNonZero(mask)
    if coords is None:
        return 0, 0, mask.shape[1], mask.shape[0]
    x, y, w, h = cv2.boundingRect(coords)
    return x, y, w, h


def crop_and_scale_to_match(capture_rgba, ref_rgba, scale_adjustment=1.0):
    """
    Iteratively find best crop, scale, and position to match reference.
    
    Strategy:
    1. Find the mech bounds in capture
    2. Scale to roughly match the reference height
    3. Center on the canvas, preferring foot alignment at the bottom
    
    Args:
        scale_adjustment: Fine-tuning multiplier for the calculated scale (default 0.95)
    
    Returns: (matted_result, params_dict)
    """
    # Get masks
    cap_mask = capture_rgba[:, :, 3]
    ref_mask = ref_rgba[:, :, 3]
    
    # Find bounds
    cap_x, cap_y, cap_w, cap_h = find_content_bounds(cap_mask)
    ref_x, ref_y, ref_w, ref_h = find_content_bounds(ref_mask)
    
    print(f"Capture bounds: x={cap_x}, y={cap_y}, w={cap_w}, h={cap_h}")
    print(f"Reference bounds: x={ref_x}, y={ref_y}, w={ref_w}, h={ref_h}")
    
    # Crop to content
    cropped = capture_rgba[cap_y:cap_y+cap_h, cap_x:cap_x+cap_w]
    
    # Calculate scale to match reference height
    # Apply adjustment factor for fine-tuning
    scale = (ref_h / cap_h) * scale_adjustment
    
    print(f"Calculated scale: {scale:.3f} (adjustment: {scale_adjustment})")
    
    # Scale the cropped mech
    new_w = int(cap_w * scale)
    new_h = int(cap_h * scale)
    scaled = cv2.resize(cropped, (new_w, new_h), interpolation=cv2.INTER_LANCZOS4)
    
    # Create canvas
    canvas = np.zeros((CANVAS_HEIGHT, CANVAS_WIDTH, 4), dtype=np.uint8)
    
    # Position: center horizontally, align feet at bottom
    ref_bottom_margin = CANVAS_HEIGHT - (ref_y + ref_h)
    print(f"Reference bottom margin: {ref_bottom_margin}px")
    
    # Place the scaled mech
    # X: centered
    dest_x = (CANVAS_WIDTH - new_w) // 2
    # Y: align bottom with reference bottom
    dest_y = CANVAS_HEIGHT - new_h - ref_bottom_margin
    
    # Clamp to canvas
    if dest_x < 0:
        scaled = scaled[:, -dest_x:]
        new_w = scaled.shape[1]
        dest_x = 0
    if dest_y < 0:
        scaled = scaled[-dest_y:, :]
        new_h = scaled.shape[0]
        dest_y = 0
    
    # Crop if too large
    if dest_x + new_w > CANVAS_WIDTH:
        new_w = CANVAS_WIDTH - dest_x
        scaled = scaled[:, :new_w]
    if dest_y + new_h > CANVAS_HEIGHT:
        new_h = CANVAS_HEIGHT - dest_y
        scaled = scaled[:new_h, :]
    
    # Paste onto canvas
    canvas[dest_y:dest_y+new_h, dest_x:dest_x+new_w] = scaled
    
    # Build parameters dictionary
    params = {
        "source": {
            "file": str(CAPTURE_FRAME),
            "dimensions": {
                "width": capture_rgba.shape[1],
                "height": capture_rgba.shape[0]
            }
        },
        "crop": {
            "x": int(cap_x),
            "y": int(cap_y),
            "width": int(cap_w),
            "height": int(cap_h)
        },
        "scale": float(scale),
        "scale_adjustment": float(scale_adjustment),
        "destination": {
            "x": int(dest_x),
            "y": int(dest_y),
            "canvas_width": CANVAS_WIDTH,
            "canvas_height": CANVAS_HEIGHT
        },
        "reference": {
            "file": str(PACK_IDLE),
            "content_bounds": {
                "x": int(ref_x),
                "y": int(ref_y),
                "width": int(ref_w),
                "height": int(ref_h)
            },
            "bottom_margin": int(ref_bottom_margin)
        },
        "processing": {
            "background_removal": "Threshold 25-200, UI exclusion, selective cavity removal with elliptical torso protection (25%×20%)",
            "interpolation": "LANCZOS4",
            "alignment_strategy": "feet at bottom, centered horizontally"
        }
    }
    
    return canvas, params


def create_diagnostics(matted, reference, params):
    """Generate visual diagnostics showing the match quality."""
    
    # 1. Side-by-side comparison
    sidebyside = np.zeros((CANVAS_HEIGHT, CANVAS_WIDTH * 3 + 20, 4), dtype=np.uint8)
    sidebyside[:, :CANVAS_WIDTH] = reference
    sidebyside[:, CANVAS_WIDTH+10:CANVAS_WIDTH*2+10] = matted
    
    # Difference (overlay)
    overlay = np.zeros((CANVAS_HEIGHT, CANVAS_WIDTH, 4), dtype=np.uint8)
    # Reference in green
    overlay[:, :, 1] = reference[:, :, 3]
    # Matted in red
    overlay[:, :, 2] = matted[:, :, 3]
    # Overlap appears yellow
    overlay[:, :, 3] = np.maximum(reference[:, :, 3], matted[:, :, 3])
    
    sidebyside[:, CANVAS_WIDTH*2+20:] = overlay
    
    # Add checkerboard behind everything for visibility
    checker_size = 8
    checker = np.zeros((CANVAS_HEIGHT, CANVAS_WIDTH * 3 + 20, 3), dtype=np.uint8)
    for i in range(0, CANVAS_HEIGHT, checker_size):
        for j in range(0, CANVAS_WIDTH * 3 + 20, checker_size):
            if ((i // checker_size) + (j // checker_size)) % 2:
                checker[i:i+checker_size, j:j+checker_size] = [64, 64, 64]
            else:
                checker[i:i+checker_size, j:j+checker_size] = [32, 32, 32]
    
    # Composite onto checker
    final = checker.copy()
    for i in range(3):
        alpha = sidebyside[:, :, 3:4] / 255.0
        final[:, :, i] = (sidebyside[:, :, i] * alpha[:, :, 0] + 
                          final[:, :, i] * (1 - alpha[:, :, 0]))
    
    cv2.imwrite(str(OUTPUT_DIR / "diagnostic-sidebyside.png"), final)
    
    # 2. Silhouette overlay (reference edges + matted edges)
    ref_edges = cv2.Canny(reference[:, :, 3], 50, 150)
    mat_edges = cv2.Canny(matted[:, :, 3], 50, 150)
    
    edges_img = np.zeros((CANVAS_HEIGHT, CANVAS_WIDTH, 3), dtype=np.uint8)
    edges_img[ref_edges > 0] = [0, 255, 0]  # Green for reference
    edges_img[mat_edges > 0] = [0, 0, 255]  # Red for matted
    # Overlap appears as combined color
    
    cv2.imwrite(str(OUTPUT_DIR / "diagnostic-edges.png"), edges_img)
    
    # 3. Individual outputs for inspection
    # Matted on checker
    matted_viz = checker[:, :CANVAS_WIDTH].copy()
    for i in range(3):
        alpha = matted[:, :, 3:4] / 255.0
        matted_viz[:, :, i] = (matted[:, :, i] * alpha[:, :, 0] + 
                               matted_viz[:, :, i] * (1 - alpha[:, :, 0]))
    cv2.imwrite(str(OUTPUT_DIR / "diagnostic-matted.png"), matted_viz)
    
    # Reference on checker
    ref_viz = checker[:, :CANVAS_WIDTH].copy()
    for i in range(3):
        alpha = reference[:, :, 3:4] / 255.0
        ref_viz[:, :, i] = (reference[:, :, i] * alpha[:, :, 0] + 
                            ref_viz[:, :, i] * (1 - alpha[:, :, 0]))
    cv2.imwrite(str(OUTPUT_DIR / "diagnostic-reference.png"), ref_viz)
    
    print("\nDiagnostics saved:")
    print(f"  - diagnostic-sidebyside.png (ref | matted | overlay)")
    print(f"  - diagnostic-edges.png (edge overlay)")
    print(f"  - diagnostic-matted.png (matted result)")
    print(f"  - diagnostic-reference.png (pack reference)")


def main():
    print("=== Timber Wolf Garrison Frame 0 Lock ===\n")
    
    # Load images
    print("Loading images...")
    capture = cv2.imread(str(CAPTURE_FRAME), cv2.IMREAD_UNCHANGED)
    reference = cv2.imread(str(PACK_IDLE), cv2.IMREAD_UNCHANGED)
    
    if capture is None or reference is None:
        print("Error: Could not load images")
        return 1
    
    print(f"Capture: {capture.shape[1]}×{capture.shape[0]}")
    print(f"Reference: {reference.shape[1]}×{reference.shape[0]}\n")
    
    # Remove background
    print("Removing background from capture...")
    capture_rgba, mask = remove_background(capture)
    
    # Save the cleaned capture for inspection
    cv2.imwrite(str(OUTPUT_DIR / "diagnostic-capture-cleaned.png"), capture_rgba)
    print(f"Saved cleaned capture to diagnostic-capture-cleaned.png\n")
    
    # Find best match
    print("Finding optimal crop/scale/position...")
    matted, params = crop_and_scale_to_match(capture_rgba, reference)
    
    # Save lock parameters
    print(f"\nSaving lock parameters to {LOCK_JSON.name}...")
    with open(LOCK_JSON, 'w') as f:
        json.dump(params, f, indent=2)
    
    print("\nLock parameters:")
    print(f"  Crop: ({params['crop']['x']}, {params['crop']['y']}) "
          f"{params['crop']['width']}×{params['crop']['height']}")
    print(f"  Scale: {params['scale']:.3f}")
    print(f"  Position: ({params['destination']['x']}, {params['destination']['y']})")
    
    # Generate diagnostics
    print("\nGenerating diagnostics...")
    create_diagnostics(matted, reference, params)
    
    # Calculate match metrics
    ref_mask = reference[:, :, 3]
    mat_mask = matted[:, :, 3]
    
    intersection = np.sum((ref_mask > 0) & (mat_mask > 0))
    union = np.sum((ref_mask > 0) | (mat_mask > 0))
    iou = intersection / union if union > 0 else 0
    
    ref_pixels = np.sum(ref_mask > 0)
    mat_pixels = np.sum(mat_mask > 0)
    
    print(f"\nMatch metrics:")
    print(f"  IoU (Intersection over Union): {iou:.3f}")
    print(f"  Reference pixels: {ref_pixels}")
    print(f"  Matted pixels: {mat_pixels}")
    print(f"  Pixel ratio: {mat_pixels/ref_pixels:.3f}")
    
    print("\n✓ Frame 0 lock complete!")
    print(f"  Lock params: {LOCK_JSON.name}")
    print(f"  Diagnostics: diagnostic-*.png")
    
    return 0


if __name__ == "__main__":
    exit(main())
