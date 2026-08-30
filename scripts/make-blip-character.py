#!/usr/bin/env python3
"""Draw Blip's art.

Not art. A stand-in that gives the renderer eight visibly different Animations
to play and the hit-test something with transparent regions to test against.
Real Characters are drawn by hand; this exists so the engine has a Character at
all before anyone has drawn one.

Pure standard library — zlib and struct write a PNG in a dozen lines, and a
build step that needs Pillow installed is a build step that stops working.

    python3 scripts/make-blip-character.py

It rewrites characters/blip/frames/ in place.
"""

import pathlib
import struct
import zlib

SIZE = 32
OUT = pathlib.Path(__file__).resolve().parent.parent / "characters" / "blip" / "frames"

BODY = (0x4A, 0x90, 0xD9, 255)
EDGE = (0x1C, 0x3F, 0x66, 255)
WHITE = (0xFF, 0xFF, 0xFF, 255)
PUPIL = (0x10, 0x18, 0x20, 255)
ACCENT = (0xF0, 0xC4, 0x19, 255)
CLEAR = (0, 0, 0, 0)


def blank():
    return [[CLEAR] * SIZE for _ in range(SIZE)]


def put(px, x, y, color):
    if 0 <= x < SIZE and 0 <= y < SIZE:
        px[y][x] = color


def rect(px, x0, y0, w, h, color):
    for y in range(y0, y0 + h):
        for x in range(x0, x0 + w):
            put(px, x, y, color)


def ellipse(px, cx, cy, rx, ry, fill, edge):
    """A filled ellipse with a one-pixel border, drawn on the pixel grid."""
    for y in range(cy - ry - 1, cy + ry + 2):
        for x in range(cx - rx - 1, cx + rx + 2):
            d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if d <= 1.0:
                put(px, x, y, fill)
            elif d <= 1.45:
                put(px, x, y, edge)


def eyes(px, cx, cy, look=(0, 0), shut=False):
    for side in (-4, 4):
        if shut:
            rect(px, cx + side - 2, cy, 4, 1, PUPIL)
        else:
            rect(px, cx + side - 2, cy - 2, 4, 4, WHITE)
            rect(px, cx + side - 1 + look[0], cy - 1 + look[1], 2, 2, PUPIL)


def feet(px, cx, y, spread):
    """Two stubs below the body, so a walk cycle is legible at 32 pixels."""
    rect(px, cx - spread - 2, y, 4, 3, EDGE)
    rect(px, cx + spread - 2, y, 4, 3, EDGE)


def mouth(px, cx, y, open_px):
    if open_px <= 0:
        rect(px, cx - 3, y, 6, 1, PUPIL)
    else:
        rect(px, cx - 3, y, 6, open_px, PUPIL)


def body(bob=0, squash=0, look=(0, 0), shut=False, spread=5, open_mouth=0):
    """The Character: one blob, posed."""
    px = blank()
    cy = 18 + bob
    # Squashing takes height off and gives back half of it in width, which reads
    # as weight. Giving it back one-for-one turns the Character into a pancake.
    ry = 8 - squash
    rx = 9 + squash // 2
    ellipse(px, 16, cy, rx, ry, BODY, EDGE)
    feet(px, 16, cy + ry + 1, spread)
    eyes(px, 16, cy - 2, look, shut)
    mouth(px, 16, cy + 3, open_mouth)
    return px


def png(px):
    raw = b"".join(
        b"\x00" + b"".join(struct.pack("BBBB", *px[y][x]) for x in range(SIZE))
        for y in range(SIZE)
    )

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def zed(px, x, y):
    """A sleeping Z, so `sleep` is unmistakable at a glance."""
    rect(px, x, y, 5, 1, ACCENT)
    rect(px, x, y + 4, 5, 1, ACCENT)
    for i in range(3):
        put(px, x + 3 - i, y + 1 + i, ACCENT)


def bang(px, x, y):
    """An exclamation mark, so `react` is unmistakable at a glance."""
    rect(px, x, y, 2, 5, ACCENT)
    rect(px, x, y + 6, 2, 2, ACCENT)


def animations():
    """Every Animation, as a list of frames. Poses, not art direction."""
    out = {}

    out["idle"] = [body(bob=0), body(bob=1)]

    # A walk reads as a walk because the feet part and the body rises.
    out["walk"] = [
        body(bob=0, spread=5),
        body(bob=-1, spread=2),
        body(bob=0, spread=5),
        body(bob=-1, spread=8),
    ]

    # Falling: stretched, feet together, eyes wide and looking down.
    out["fall"] = [
        body(bob=0, squash=-2, spread=1, look=(0, 1)),
        body(bob=1, squash=-2, spread=1, look=(0, 1)),
    ]

    # Landing: squashed flat, then most of the way back. Plays once.
    out["land"] = [
        body(bob=4, squash=5, spread=7, shut=True),
        body(bob=2, squash=2, spread=6),
        body(bob=0, squash=0, spread=5),
    ]

    out["sit"] = [
        body(bob=3, squash=2, spread=7),
        body(bob=3, squash=2, spread=7, look=(0, 1)),
    ]

    sleep = []
    for offset in (0, 2):
        px = body(bob=3, squash=2, spread=7, shut=True)
        zed(px, 24, 4 - offset)
        sleep.append(px)
    out["sleep"] = sleep

    react = []
    for lift in (0, 2, 1):
        px = body(bob=-lift, look=(0, -1))
        bang(px, 26, 2)
        react.append(px)
    out["react"] = react

    out["talk"] = [body(open_mouth=n) for n in (0, 2, 3, 1)]
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for existing in OUT.glob("*.png"):
        existing.unlink()

    for name, frames in animations().items():
        for index, px in enumerate(frames):
            path = OUT / f"{name}-{index}.png"
            path.write_bytes(png(px))
            print(path.relative_to(OUT.parent.parent.parent))


if __name__ == "__main__":
    main()
