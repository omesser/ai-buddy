#!/usr/bin/env python3
"""Generate looping GIFs from character frames for the README."""

import os
from pathlib import Path
from PIL import Image


def create_looping_gif(frames, output_path, fps, scale=1):
    """Create a looping GIF from a list of frame paths.

    Args:
        frames: List of paths to PNG frames
        output_path: Where to save the GIF
        fps: Frames per second
        scale: Integer scale factor (using nearest neighbor)
    """
    images = []
    for frame_path in frames:
        img = Image.open(frame_path)
        if scale > 1:
            new_size = (img.width * scale, img.height * scale)
            img = img.resize(new_size, Image.NEAREST)
        images.append(img)

    duration_ms = int(1000 / fps)

    images[0].save(
        output_path,
        save_all=True,
        append_images=images[1:],
        duration=duration_ms,
        loop=0,
        optimize=False,
        disposal=2,
    )


def main():
    repo_root = Path(__file__).parent.parent
    chars_dir = repo_root / "characters"
    out_dir = repo_root / "docs" / "readme"
    out_dir.mkdir(parents=True, exist_ok=True)

    # Buddy Bot: idle animation (smooth render, frames at authored 90×90 size)
    # Use the main idle hover animation
    buddy_idle_frames = [
        chars_dir / "buddy-bot" / "frames" / f"idle-{i}.png" for i in range(16)
    ]
    create_looping_gif(
        buddy_idle_frames, out_dir / "buddy-bot-idle.gif", fps=5, scale=1
    )
    print(f"✓ buddy-bot-idle.gif ({os.path.getsize(out_dir / 'buddy-bot-idle.gif') / 1024:.1f} KB)")

    # Buddy Bot: react animation (delighted startle)
    buddy_react_frames = [
        chars_dir / "buddy-bot" / "frames" / f"react-{i}.png" for i in range(5)
    ]
    create_looping_gif(
        buddy_react_frames, out_dir / "buddy-bot-react.gif", fps=8, scale=1
    )
    print(f"✓ buddy-bot-react.gif ({os.path.getsize(out_dir / 'buddy-bot-react.gif') / 1024:.1f} KB)")

    # Nim: idle animation (pixel art, scale 2x for visibility)
    nim_idle_frames = [
        chars_dir / "nim" / "frames" / f"idle-{i}.png" for i in range(6)
    ]
    create_looping_gif(nim_idle_frames, out_dir / "nim-idle.gif", fps=8, scale=2)
    print(f"✓ nim-idle.gif ({os.path.getsize(out_dir / 'nim-idle.gif') / 1024:.1f} KB)")

    # Black Mage: idle animation (already scaled 3x in source, 3 frames)
    black_mage_idle_frames = [
        chars_dir / "black-mage" / "frames" / f"idle-{i}.png" for i in range(3)
    ]
    create_looping_gif(
        black_mage_idle_frames, out_dir / "black-mage-idle.gif", fps=1, scale=1
    )
    print(f"✓ black-mage-idle.gif ({os.path.getsize(out_dir / 'black-mage-idle.gif') / 1024:.1f} KB)")

    print(f"\nGIFs generated in {out_dir}/")
    print("Transparent backgrounds preserved, nearest-neighbor scaling, looping enabled.")


if __name__ == "__main__":
    main()
