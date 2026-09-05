#!/usr/bin/env python3
"""Generate looping GIFs from character frames for the README."""

import os
from pathlib import Path
from PIL import Image


def create_looping_gif(frames, output_path, fps, scale=1, target_height=None):
    """Create a looping GIF from a list of frame paths.

    Args:
        frames: List of paths to PNG frames
        output_path: Where to save the GIF
        fps: Frames per second
        scale: Integer scale factor (using nearest neighbor)
        target_height: If set, scale to this height (overrides scale)
    """
    images = []
    for frame_path in frames:
        img = Image.open(frame_path)
        
        if target_height:
            # Scale to target height, maintaining aspect ratio
            aspect_ratio = img.width / img.height
            new_height = target_height
            new_width = int(target_height * aspect_ratio)
            img = img.resize((new_width, new_height), Image.NEAREST)
        elif scale > 1:
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

    # Target height for consistent visual sizing in README
    target_height = 96

    # Buddy Bot: idle animation (smooth render, 16 frames)
    buddy_idle_frames = [
        chars_dir / "buddy-bot" / "frames" / f"idle-{i}.png" for i in range(16)
    ]
    create_looping_gif(
        buddy_idle_frames, out_dir / "buddy-bot-idle.gif", fps=5, target_height=target_height
    )
    print(f"✓ buddy-bot-idle.gif ({os.path.getsize(out_dir / 'buddy-bot-idle.gif') / 1024:.1f} KB)")

    # Buddy Bot: react animation (delighted startle, 5 frames)
    buddy_react_frames = [
        chars_dir / "buddy-bot" / "frames" / f"react-{i}.png" for i in range(5)
    ]
    create_looping_gif(
        buddy_react_frames, out_dir / "buddy-bot-react.gif", fps=8, target_height=target_height
    )
    print(f"✓ buddy-bot-react.gif ({os.path.getsize(out_dir / 'buddy-bot-react.gif') / 1024:.1f} KB)")

    # Nim: idle animation (pixel art, 6 frames)
    nim_idle_frames = [
        chars_dir / "nim" / "frames" / f"idle-{i}.png" for i in range(6)
    ]
    create_looping_gif(
        nim_idle_frames, out_dir / "nim-idle.gif", fps=8, target_height=target_height
    )
    print(f"✓ nim-idle.gif ({os.path.getsize(out_dir / 'nim-idle.gif') / 1024:.1f} KB)")

    # Black Mage: idle animation (3 frames, already scaled 3x in source)
    black_mage_idle_frames = [
        chars_dir / "black-mage" / "frames" / f"idle-{i}.png" for i in range(3)
    ]
    create_looping_gif(
        black_mage_idle_frames, out_dir / "black-mage-idle.gif", fps=1, target_height=target_height
    )
    print(f"✓ black-mage-idle.gif ({os.path.getsize(out_dir / 'black-mage-idle.gif') / 1024:.1f} KB)")

    # BMO: idle animation (2 frames, smooth render)
    bmo_idle_frames = [
        chars_dir / "bmo" / "frames" / f"idle-{i}.png" for i in range(2)
    ]
    create_looping_gif(
        bmo_idle_frames, out_dir / "bmo-idle.gif", fps=1, target_height=target_height
    )
    print(f"✓ bmo-idle.gif ({os.path.getsize(out_dir / 'bmo-idle.gif') / 1024:.1f} KB)")

    # Cat: idle animation (6 frames)
    cat_idle_frames = [
        chars_dir / "cat" / "frames" / f"idle-{i}.png" for i in range(6)
    ]
    create_looping_gif(
        cat_idle_frames, out_dir / "cat-idle.gif", fps=8, target_height=target_height
    )
    print(f"✓ cat-idle.gif ({os.path.getsize(out_dir / 'cat-idle.gif') / 1024:.1f} KB)")

    # Jotaro Kujo: idle animation (2 frames)
    jotaro_idle_frames = [
        chars_dir / "jotaro-kujo" / "frames" / f"idle-{i}.png" for i in range(2)
    ]
    create_looping_gif(
        jotaro_idle_frames, out_dir / "jotaro-idle.gif", fps=2, target_height=target_height
    )
    print(f"✓ jotaro-idle.gif ({os.path.getsize(out_dir / 'jotaro-idle.gif') / 1024:.1f} KB)")

    # Timber Wolf: idle animation (2 frames)
    timber_idle_frames = [
        chars_dir / "timber-wolf" / "frames" / f"idle-{i}.png" for i in range(2)
    ]
    create_looping_gif(
        timber_idle_frames, out_dir / "timber-wolf-idle.gif", fps=8, target_height=target_height
    )
    print(f"✓ timber-wolf-idle.gif ({os.path.getsize(out_dir / 'timber-wolf-idle.gif') / 1024:.1f} KB)")

    # Trump: idle animation (6 frames)
    trump_idle_frames = [
        chars_dir / "trump" / "frames" / f"idle-{i}.png" for i in range(6)
    ]
    create_looping_gif(
        trump_idle_frames, out_dir / "trump-idle.gif", fps=8, target_height=target_height
    )
    print(f"✓ trump-idle.gif ({os.path.getsize(out_dir / 'trump-idle.gif') / 1024:.1f} KB)")

    print(f"\nAll GIFs generated in {out_dir}/")
    print(f"Target height: {target_height}px for consistent visual sizing")
    print("Transparent backgrounds preserved, nearest-neighbor scaling, looping enabled.")


if __name__ == "__main__":
    main()
