#!/usr/bin/env python3
"""Tests for the petscodex pet importer.

This test suite covers importing petscodex/petdex packages into Character
Packages with the Required Animation Set, variant rings, and per-animation
union bbox cropping.
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))
try:
    import import_pet
except (ImportError, ModuleNotFoundError):
    import_pet = None


def make_png(width, height, color=(255, 0, 0, 255)):
    """Generate a minimal PNG of the given size using stdlib only."""
    import struct
    import zlib

    raw = b"".join(
        b"\x00" + b"".join(struct.pack("BBBB", *color) for _ in range(width))
        for _ in range(height)
    )

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def make_petscodex_atlas(rows=9, cols=8, cell_width=192, cell_height=208, colors=None):
    """Generate a synthetic petdex spritesheet with distinct per-row colors.

    Args:
        rows: Number of rows (9 for v1, 11 for v2)
        cols: Number of columns (always 8)
        cell_width: Width of each cell
        cell_height: Height of each cell
        colors: Optional list of (r, g, b, a) tuples for each row

    Returns:
        PNG bytes
    """
    import struct
    import zlib

    if colors is None:
        colors = [
            (255, 0, 0, 255),    # row 0 idle - red
            (0, 255, 0, 255),    # row 1 running-right - green
            (0, 0, 255, 255),    # row 2 running-left - blue
            (255, 255, 0, 255),  # row 3 waving - yellow
            (255, 0, 255, 255),  # row 4 jumping - magenta
            (0, 255, 255, 255),  # row 5 failed - cyan
            (128, 0, 0, 255),    # row 6 waiting - dark red
            (0, 128, 0, 255),    # row 7 running - dark green
            (0, 0, 128, 255),    # row 8 review - dark blue
            (128, 128, 0, 255),  # row 9 (v2)
            (128, 0, 128, 255),  # row 10 (v2)
        ]

    width = cell_width * cols
    height = cell_height * rows

    raw_lines = []
    for y in range(height):
        row_idx = y // cell_height
        color = colors[row_idx] if row_idx < len(colors) else (128, 128, 128, 255)
        line = b"\x00" + b"".join(struct.pack("BBBB", *color) for _ in range(width))
        raw_lines.append(line)

    raw = b"".join(raw_lines)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


class TestPetscodexMapping(unittest.TestCase):
    """Test that a synthetic petdex atlas maps to all nine required Animations."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)

    def test_synthetic_atlas_maps_to_required_animations(self):
        """A synthetic 8×9 PNG atlas with per-row colors maps to all nine required animation names."""
        if import_pet is None:
            self.skipTest("import_pet module not available")

        pet_dir = Path(self.temp_dir.name) / "pet"
        pet_dir.mkdir()

        spritesheet = pet_dir / "spritesheet.png"
        spritesheet.write_bytes(make_petscodex_atlas())

        pet_json = pet_dir / "pet.json"
        pet_json.write_text(json.dumps({
            "id": "test-cat",
            "displayName": "Test Cat",
            "description": "A test cat",
            "spritesheetPath": "spritesheet.png",
            "pet_license": "MIT"
        }))

        out_dir = Path(self.temp_dir.name) / "output"
        import_pet.import_petscodex(pet_dir, out_dir)

        manifest = out_dir / "character.manifest"
        self.assertTrue(manifest.exists(), "character.manifest was created")

        manifest_text = manifest.read_text()

        required = ["idle", "walk", "fall", "land", "sit", "sleep", "react", "talk", "hold"]
        for anim in required:
            self.assertIn(f"[animations.{anim}]", manifest_text,
                         f"manifest declares {anim}")

    def test_waiting_is_variant_of_idle(self):
        """waiting becomes variant_of = idle; land and react are loop = once."""
        if import_pet is None:
            self.skipTest("import_pet module not available")

        pet_dir = Path(self.temp_dir.name) / "pet"
        pet_dir.mkdir()

        spritesheet = pet_dir / "spritesheet.png"
        spritesheet.write_bytes(make_petscodex_atlas())

        pet_json = pet_dir / "pet.json"
        pet_json.write_text(json.dumps({
            "id": "test-cat",
            "displayName": "Test Cat",
            "description": "A test cat",
            "spritesheetPath": "spritesheet.png",
            "pet_license": "MIT"
        }))

        out_dir = Path(self.temp_dir.name) / "output"
        import_pet.import_petscodex(pet_dir, out_dir)

        manifest = out_dir / "character.manifest"
        manifest_text = manifest.read_text()

        self.assertIn('[animations.waiting]', manifest_text)
        self.assertIn('variant_of = "idle"', manifest_text)

        land_section = manifest_text[manifest_text.find('[animations.land]'):]
        land_section = land_section[:land_section.find('\n[animations.') if '\n[animations.' in land_section else len(land_section)]
        self.assertIn('loop = "once"', land_section)

        react_section = manifest_text[manifest_text.find('[animations.react]'):]
        react_section = react_section[:react_section.find('\n[animations.') if '\n[animations.' in react_section else len(react_section)]
        self.assertIn('loop = "once"', react_section)

    def test_fps_from_duration(self):
        """fps = round(frames * 1000 / durationMs), clamped to loader bounds."""
        if import_pet is None:
            self.skipTest("import_pet module not available")

        pet_dir = Path(self.temp_dir.name) / "pet"
        pet_dir.mkdir()

        spritesheet = pet_dir / "spritesheet.png"
        spritesheet.write_bytes(make_petscodex_atlas())

        pet_json = pet_dir / "pet.json"
        pet_json.write_text(json.dumps({
            "id": "test-cat",
            "displayName": "Test Cat",
            "description": "A test cat",
            "spritesheetPath": "spritesheet.png",
            "pet_license": "MIT"
        }))

        out_dir = Path(self.temp_dir.name) / "output"
        import_pet.import_petscodex(pet_dir, out_dir)

        manifest = out_dir / "character.manifest"
        manifest_text = manifest.read_text()

        idle_section = manifest_text[manifest_text.find('[animations.idle]'):]
        idle_section = idle_section[:idle_section.find('\n[animations.') if '\n[animations.' in idle_section else len(idle_section)]
        self.assertIn('fps = ', idle_section)

        fps_line = [line for line in idle_section.split('\n') if line.startswith('fps = ')][0]
        fps = int(fps_line.split('=')[1].strip())
        expected_fps = round(6 * 1000 / 1100)
        self.assertEqual(fps, expected_fps)

    def test_union_bbox_crop_not_per_frame(self):
        """Per-animation union bbox: frames share canvas size; baseline not flattened per-frame."""
        if import_pet is None:
            self.skipTest("import_pet module not available")

        pet_dir = Path(self.temp_dir.name) / "pet"
        pet_dir.mkdir()

        spritesheet = pet_dir / "spritesheet.png"
        spritesheet.write_bytes(make_petscodex_atlas())

        pet_json = pet_dir / "pet.json"
        pet_json.write_text(json.dumps({
            "id": "test-cat",
            "displayName": "Test Cat",
            "description": "A test cat",
            "spritesheetPath": "spritesheet.png",
            "pet_license": "MIT"
        }))

        out_dir = Path(self.temp_dir.name) / "output"
        import_pet.import_petscodex(pet_dir, out_dir)

        frames_dir = out_dir / "frames"
        self.assertTrue(frames_dir.exists())

        walk_frames = sorted(frames_dir.glob("walk-*.png"))
        self.assertGreater(len(walk_frames), 0)

        import struct
        def png_size(path):
            data = path.read_bytes()
            return struct.unpack('>II', data[16:24])

        sizes = [png_size(f) for f in walk_frames]
        self.assertEqual(len(set(sizes)), 1, "all walk frames have the same dimensions")

    def test_unknown_license_without_flag_fails(self):
        """Unknown license without --accept-license fails; with the flag it writes."""
        if import_pet is None:
            self.skipTest("import_pet module not available")

        pet_dir = Path(self.temp_dir.name) / "pet"
        pet_dir.mkdir()

        spritesheet = pet_dir / "spritesheet.png"
        spritesheet.write_bytes(make_petscodex_atlas())

        pet_json = pet_dir / "pet.json"
        pet_json.write_text(json.dumps({
            "id": "test-cat",
            "displayName": "Test Cat",
            "description": "A test cat",
            "spritesheetPath": "spritesheet.png",
            "pet_license": "UNKNOWN-LICENSE"
        }))

        out_dir = Path(self.temp_dir.name) / "output"

        with self.assertRaises(SystemExit) as cm:
            import_pet.import_petscodex(pet_dir, out_dir, accept_license=False)

        self.assertEqual(cm.exception.code, 1)
        self.assertFalse((out_dir / "character.manifest").exists(),
                        "manifest should not exist after license rejection")

        out_dir_accepted = Path(self.temp_dir.name) / "output_accepted"
        import_pet.import_petscodex(pet_dir, out_dir_accepted, accept_license=True)
        self.assertTrue((out_dir_accepted / "character.manifest").exists())


if __name__ == "__main__":
    unittest.main()
