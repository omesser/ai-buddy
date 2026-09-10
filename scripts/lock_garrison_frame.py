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
    Remove background from capture frame.
    Uses color-based segmentation to isolate the mech from the dark background and UI.
    """
    if img.shape[2] == 4:
        rgb = img[:, :, :3]
        alpha = img[:, :, 3]
    else:
        rgb = img
        alpha = np.ones((img.shape[0], img.shape[1]), dtype=np.uint8) * 255
    
    # Convert to grayscale for thresholding
    gray = cv2.cvtColor(rgb, cv2.COLOR_BGR2GRAY)
    
    # The mech has distinct colors (browns, metallics) that are brighter than pure black
    # Use a higher threshold to exclude the very dark background
    _, mask = cv2.threshold(gray, 30, 255, cv2.THRESH_BINARY)
    
    # Additional filtering: exclude very blue/white UI elements
    # The mech is mostly browns/grays, not bright whites or blues
    hsv = cv2.cvtColor(rgb, cv2.COLOR_BGR2HSV)
    # Exclude very bright whites (UI chrome)
    bright_mask = hsv[:, :, 2] < 220
    mask = cv2.bitwise_and(mask, mask, mask=bright_mask.astype(np.uint8) * 255)
    
    # Clean up with morphological operations
    kernel = np.ones((5, 5), np.uint8)
    mask = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, kernel, iterations=3)
    mask = cv2.morphologyEx(mask, cv2.MORPH_OPEN, kernel, iterations=2)
    
    # Find contours and keep only substantial ones (exclude small UI bits)
    contours, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    
    if contours:
        # Filter by area - keep only large contours
        total_area = img.shape[0] * img.shape[1]
        min_area = total_area * 0.02  # At least 2% of frame
        
        large_contours = [c for c in contours if cv2.contourArea(c) > min_area]
        
        if large_contours:
            # Find the most centered contour (likely the mech)
            img_center_x = img.shape[1] / 2
            img_center_y = img.shape[0] / 2
            
            def contour_center_distance(contour):
                M = cv2.moments(contour)
                if M["m00"] == 0:
                    return float('inf')
                cx = M["m10"] / M["m00"]
                cy = M["m01"] / M["m00"]
                return np.sqrt((cx - img_center_x)**2 + (cy - img_center_y)**2)
            
            # Use the most centered large contour
            mech_contour = min(large_contours, key=contour_center_distance)
            
            mask = np.zeros_like(mask)
            cv2.drawContours(mask, [mech_contour], -1, 255, -1)
    
    # Final cleanup - fill any holes in the mech
    kernel = np.ones((7, 7), np.uint8)
    mask = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, kernel, iterations=2)
    
    # Create RGBA output
    result = np.zeros((img.shape[0], img.shape[1], 4), dtype=np.uint8)
    result[:, :, :3] = rgb
    result[:, :, 3] = mask
    
    return result, mask


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
            "background_removal": "threshold at 30, HSV filtering, centroid selection, morphological cleanup",
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
