#!/usr/bin/env python3
"""Generate showcase GIFs from character frames for the README.
Creates more interesting animations (not boring idles) for the Characters table."""

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

    buddy_walk_frames = [
        chars_dir / "buddy-bot" / "frames" / f"walk-{i}.png" for i in range(8)
    ]
    create_looping_gif(
        buddy_walk_frames, out_dir / "buddy-bot-walk.gif", fps=8, target_height=target_height
    )
    print(f"✓ buddy-bot-walk.gif ({os.path.getsize(out_dir / 'buddy-bot-walk.gif') / 1024:.1f} KB)")

    buddy_react_frames = [
        chars_dir / "buddy-bot" / "frames" / f"react-{i}.png" for i in range(5)
    ]
    create_looping_gif(
        buddy_react_frames, out_dir / "buddy-bot-react.gif", fps=12, target_height=target_height
    )
    print(f"✓ buddy-bot-react.gif ({os.path.getsize(out_dir / 'buddy-bot-react.gif') / 1024:.1f} KB)")

    black_mage_talk_frames = [
        chars_dir / "black-mage" / "frames" / f"talk-{i}.png" for i in range(2)
    ]
    create_looping_gif(
        black_mage_talk_frames, out_dir / "black-mage-talk.gif", fps=3, target_height=target_height
    )
    print(f"✓ black-mage-talk.gif ({os.path.getsize(out_dir / 'black-mage-talk.gif') / 1024:.1f} KB)")

    bmo_sing_frames = [
        chars_dir / "bmo" / "frames" / f"sing-{i}.png" for i in range(4)
    ]
    create_looping_gif(
        bmo_sing_frames, out_dir / "bmo-sing.gif", fps=3, target_height=target_height
    )
    print(f"✓ bmo-sing.gif ({os.path.getsize(out_dir / 'bmo-sing.gif') / 1024:.1f} KB)")

    cat_walk_frames = [
        chars_dir / "cat" / "frames" / f"walk-{i}.png" for i in range(8)
    ]
    create_looping_gif(
        cat_walk_frames, out_dir / "cat-walk.gif", fps=8, target_height=target_height
    )
    print(f"✓ cat-walk.gif ({os.path.getsize(out_dir / 'cat-walk.gif') / 1024:.1f} KB)")

    jotaro_react_frames = [
        chars_dir / "jotaro-kujo" / "frames" / f"react-{i}.png" for i in range(8)
    ]
    create_looping_gif(
        jotaro_react_frames, out_dir / "jotaro-kujo-react.gif", fps=7, target_height=target_height
    )
    print(f"✓ jotaro-kujo-react.gif ({os.path.getsize(out_dir / 'jotaro-kujo-react.gif') / 1024:.1f} KB)")

    nim_sleep_frames = [
        chars_dir / "nim" / "frames" / f"sleep-{i}.png" for i in range(4)
    ]
    create_looping_gif(
        nim_sleep_frames, out_dir / "nim-sleep.gif", fps=3, target_height=target_height
    )
    print(f"✓ nim-sleep.gif ({os.path.getsize(out_dir / 'nim-sleep.gif') / 1024:.1f} KB)")

    # Permission granted by MekaRamen (Yeoh Kher En) to publish (#388)
    timber_wolf_walk_frames = [
        chars_dir / "timber-wolf" / "frames" / f"walk-{i}.png" for i in range(20)
    ]
    create_looping_gif(
        timber_wolf_walk_frames, out_dir / "timber-wolf-walk.gif", fps=8, target_height=target_height
    )
    print(f"✓ timber-wolf-walk.gif ({os.path.getsize(out_dir / 'timber-wolf-walk.gif') / 1024:.1f} KB)")

    trump_talk_frames = [
        chars_dir / "trump" / "frames" / f"talk-{i}.png" for i in range(3)
    ]
    create_looping_gif(
        trump_talk_frames, out_dir / "trump-talk.gif", fps=4, target_height=target_height
    )
    print(f"✓ trump-talk.gif ({os.path.getsize(out_dir / 'trump-talk.gif') / 1024:.1f} KB)")

    print(f"\nAll showcase GIFs generated in {out_dir}/")
    print(f"Target height: {target_height}px for consistent visual sizing")
    print("More interesting animations selected (walk/react/sing/scan/talk vs boring idles)")


if __name__ == "__main__":
    main()
