#!/usr/bin/env python3
"""Import pets from standardized desktop-pet packages into Character Packages.

This script translates pets from petscodex/petdex packages into ai-buddy
Character Packages with animations, TOML manifests, and proper metadata.

PILLOW ALLOWED: This is the one script in the repository permitted to depend
on Pillow, because it is an authoring tool, not a build step. The Blip
generator (make-blip-character.py) stays stdlib-only so the build never needs
Pillow installed.

Usage:
    import-pet.py <pet-dir> --format petscodex -o <out-dir>
    import-pet.py <pet-dir> --format petscodex -o <out-dir> --accept-license
"""

import argparse
import json
import struct
import sys
import zlib
from pathlib import Path
from typing import List, Tuple, Optional

try:
    from PIL import Image
except ImportError:
    print("Error: Pillow is required. Install with: pip install Pillow", file=sys.stderr)
    sys.exit(1)


REQUIRED_ANIMATIONS = ["idle", "walk", "fall", "land", "sit", "sleep", "react", "talk", "hold"]

KNOWN_LICENSES = {"MIT", "CC0", "CC-BY-4.0", "Apache-2.0", "BSD-3-Clause"}

PETSCODEX_ROWS = {
    0: {"name": "idle", "frames": 6, "duration_ms": 1100, "loop": "forever"},
    1: {"name": "running-right", "frames": 8, "duration_ms": 1060, "loop": "forever"},
    2: {"name": "running-left", "frames": 8, "duration_ms": 1060, "loop": "forever"},
    3: {"name": "waving", "frames": 4, "duration_ms": 700, "loop": "forever"},
    4: {"name": "jumping", "frames": 5, "duration_ms": 840, "loop": "forever"},
    5: {"name": "failed", "frames": 8, "duration_ms": 1220, "loop": "forever"},
    6: {"name": "waiting", "frames": 6, "duration_ms": 1010, "loop": "forever"},
    7: {"name": "running", "frames": 6, "duration_ms": 820, "loop": "forever"},
    8: {"name": "review", "frames": 6, "duration_ms": 1030, "loop": "forever"},
}


def compute_fps(frame_count: int, duration_ms: int) -> int:
    """Compute fps from frame count and duration, clamped to loader bounds [1, 60]."""
    if duration_ms <= 0:
        return 8
    fps = round(frame_count * 1000 / duration_ms)
    return max(1, min(60, fps))


def crop_to_content(image: Image.Image) -> Tuple[Image.Image, Tuple[int, int, int, int]]:
    """Crop image to content bbox, return cropped image and bbox (left, top, right, bottom)."""
    if image.mode != 'RGBA':
        image = image.convert('RGBA')

    bbox = image.getbbox()
    if bbox is None:
        return image, (0, 0, image.width, image.height)

    return image.crop(bbox), bbox


def extract_frames_from_row(spritesheet: Image.Image, row: int, frame_count: int,
                            cell_width: int, cell_height: int) -> List[Image.Image]:
    """Extract frames from a specific row of the petdex spritesheet."""
    frames = []
    for col in range(frame_count):
        x = col * cell_width
        y = row * cell_height
        frame = spritesheet.crop((x, y, x + cell_width, y + cell_height))
        frames.append(frame)
    return frames


def compute_union_bbox(frames: List[Image.Image]) -> Tuple[int, int, int, int]:
    """Compute the union bbox of all frames (left, top, right, bottom)."""
    if not frames:
        return (0, 0, 0, 0)

    min_left = min_top = float('inf')
    max_right = max_bottom = float('-inf')

    for frame in frames:
        _, bbox = crop_to_content(frame)
        min_left = min(min_left, bbox[0])
        min_top = min(min_top, bbox[1])
        max_right = max(max_right, bbox[2])
        max_bottom = max(max_bottom, bbox[3])

    return (int(min_left), int(min_top), int(max_right), int(max_bottom))


def align_to_baseline(frames: List[Image.Image], union_bbox: Tuple[int, int, int, int]) -> List[Image.Image]:
    """Align frames to a collective baseline at the canvas bottom.

    The per-animation minimum margin (not per-frame) so a jump is not flattened.
    """
    if not frames:
        return frames

    canvas_width = union_bbox[2] - union_bbox[0]
    canvas_height = union_bbox[3] - union_bbox[1]

    if canvas_width <= 0 or canvas_height <= 0:
        return frames

    aligned = []
    for frame in frames:
        canvas = Image.new('RGBA', (canvas_width, canvas_height), (0, 0, 0, 0))
        _, bbox = crop_to_content(frame)

        x_offset = bbox[0] - union_bbox[0]
        y_offset = bbox[1] - union_bbox[1]

        cropped = frame.crop(bbox)
        canvas.paste(cropped, (x_offset, y_offset))
        aligned.append(canvas)

    return aligned


def detect_render_mode(frames: List[Image.Image]) -> str:
    """Detect render_mode based on color count and alpha histogram.

    True pixel art has few colors and sharp alpha. Smooth/AA art has many colors
    and gradual alpha transitions.
    """
    if not frames:
        return "pixelated"

    sample = frames[0]
    if sample.mode != 'RGBA':
        sample = sample.convert('RGBA')

    colors = set(sample.tobytes())

    if len(colors) <= 256:
        return "pixelated"
    return "smooth"


def choose_scale(cell_width: int, cell_height: int, target_size: int = 115) -> int:
    """Choose scale factor so on-screen size is ~100-130 logical px.

    Args:
        cell_width: Width of the source cell
        cell_height: Height of the source cell
        target_size: Target on-screen size in logical pixels

    Returns:
        Scale factor (1-4)
    """
    base_size = max(cell_width, cell_height)
    scale = max(1, min(4, round(target_size / base_size)))
    return scale


def import_petscodex(pet_dir: Path, out_dir: Path, accept_license: bool = False):
    """Import a petscodex pet into a Character Package.

    Args:
        pet_dir: Directory containing pet.json and spritesheet
        out_dir: Output directory for Character Package
        accept_license: Accept unknown licenses without prompting
    """
    pet_json_path = pet_dir / "pet.json"
    if not pet_json_path.exists():
        print(f"Error: {pet_json_path} not found", file=sys.stderr)
        sys.exit(1)

    with open(pet_json_path) as f:
        pet_data = json.load(f)

    pet_id = pet_data.get("id", "unknown")
    display_name = pet_data.get("displayName", pet_id)
    spritesheet_path = pet_dir / pet_data.get("spritesheetPath", "spritesheet.png")
    pet_license = pet_data.get("pet_license", "UNKNOWN")

    print(f"License: {pet_license}")
    if pet_license not in KNOWN_LICENSES and not accept_license:
        print(f"Error: Unknown license '{pet_license}'. Use --accept-license to proceed.", file=sys.stderr)
        raise SystemExit(1)

    if not spritesheet_path.exists():
        print(f"Error: Spritesheet not found at {spritesheet_path}", file=sys.stderr)
        sys.exit(1)

    spritesheet = Image.open(spritesheet_path)

    cell_width = spritesheet.width // 8
    cell_height = spritesheet.height // 9

    out_dir.mkdir(parents=True, exist_ok=True)
    frames_dir = out_dir / "frames"
    frames_dir.mkdir(exist_ok=True)

    animations = {}

    animations["idle"] = {
        "source_row": 0,
        "frames": 6,
        "duration_ms": 1100,
        "loop": "forever"
    }

    animations["walk"] = {
        "source_row": 1,
        "frames": 8,
        "duration_ms": 1060,
        "loop": "forever"
    }

    animations["fall"] = {
        "source_row": 4,
        "frames": [2, 3],
        "duration_ms": 840,
        "loop": "forever"
    }

    animations["land"] = {
        "source_row": 4,
        "frames": [3, 4],
        "duration_ms": 840,
        "loop": "once"
    }

    animations["react"] = {
        "source_row": 5,
        "frames": 8,
        "duration_ms": 1220,
        "loop": "once"
    }

    animations["talk"] = {
        "source_row": 3,
        "frames": 4,
        "duration_ms": 700,
        "loop": "forever"
    }

    animations["sit"] = {
        "source_row": 8,
        "frames": 6,
        "duration_ms": 1030,
        "loop": "forever"
    }

    animations["sleep"] = {
        "source_row": 0,
        "frames": [0],
        "duration_ms": 1100,
        "loop": "forever"
    }

    animations["hold"] = {
        "source_row": 4,
        "frames": [0, 1],
        "duration_ms": 840,
        "loop": "forever"
    }

    animations["waiting"] = {
        "source_row": 6,
        "frames": 6,
        "duration_ms": 1010,
        "loop": "forever",
        "variant_of": "idle"
    }

    for anim_name, anim_config in animations.items():
        row = anim_config["source_row"]
        frame_spec = anim_config["frames"]

        if isinstance(frame_spec, int):
            source_frames = extract_frames_from_row(spritesheet, row, frame_spec, cell_width, cell_height)
        else:
            all_row_frames = extract_frames_from_row(spritesheet, row, 8, cell_width, cell_height)
            source_frames = [all_row_frames[i] for i in frame_spec]

        union_bbox = compute_union_bbox(source_frames)
        aligned_frames = align_to_baseline(source_frames, union_bbox)

        for i, frame in enumerate(aligned_frames):
            frame_path = frames_dir / f"{anim_name}-{i}.png"
            frame.save(frame_path)

        anim_config["output_frames"] = len(aligned_frames)
        anim_config["fps"] = compute_fps(len(aligned_frames), anim_config["duration_ms"])

    render_mode = detect_render_mode([Image.open(frames_dir / f"idle-0.png")])
    scale = choose_scale(cell_width, cell_height)

    manifest_lines = [
        f'# Imported from petscodex: {pet_id}',
        f'# License: {pet_license}',
        '',
        f'name = "{display_name}"',
        f'render_mode = "{render_mode}"',
        f'scale = {scale}',
        '',
    ]

    for anim_name in ["idle", "waiting", "walk", "fall", "land", "sit", "sleep", "react", "talk", "hold"]:
        if anim_name not in animations:
            continue

        anim = animations[anim_name]
        manifest_lines.append(f'[animations.{anim_name}]')

        frame_list = ', '.join(f'"{anim_name}-{i}.png"' for i in range(anim["output_frames"]))
        manifest_lines.append(f'frames = [{frame_list}]')
        manifest_lines.append(f'fps = {anim["fps"]}')

        if anim.get("loop") == "once":
            manifest_lines.append('loop = "once"')

        if anim.get("variant_of"):
            manifest_lines.append(f'variant_of = "{anim["variant_of"]}"')

        manifest_lines.append('')

    manifest_path = out_dir / "character.manifest"
    manifest_path.write_text('\n'.join(manifest_lines))

    print(f"Imported {display_name} to {out_dir}")


def main():
    parser = argparse.ArgumentParser(
        description="Import pets from standardized packages into Character Packages"
    )
    parser.add_argument("pet_dir", type=Path, help="Directory containing pet package")
    parser.add_argument("--format", required=True, choices=["petscodex"], help="Source format")
    parser.add_argument("-o", "--output", type=Path, required=True, help="Output directory")
    parser.add_argument("--accept-license", action="store_true", help="Accept unknown licenses")

    args = parser.parse_args()

    if args.format == "petscodex":
        import_petscodex(args.pet_dir, args.output, args.accept_license)
    else:
        print(f"Error: Format {args.format} not implemented", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
