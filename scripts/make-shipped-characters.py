#!/usr/bin/env python3
"""Draw the two shipped Characters, Chip and Nim.

The two exist to prove the Character Package format against real variance, so
they are drawn by two different techniques rather than one technique twice:

  * **Chip** is faithful Win95 — the sixteen VGA colours, flat fills, the
    raised-button bevel of the era, and ordered dithering wherever a shade
    between two of those sixteen is wanted. Few frames, held hard.
  * **Nim** is modern pixel art — a shaded ramp lit from the upper left, a
    palette four times the size, translucent contact shadow, and enough
    in-between frames that the motion reads as smooth rather than stepped.

Pure standard library, as `make-blip-character.py` is and for the same
reason: a build step that needs Pillow installed is a build step that stops
working. The PNG writer is copied from there rather than shared, because a
third file to import would cost more than the twelve lines it saves.

    python3 scripts/make-shipped-characters.py

It rewrites characters/chip/frames/ and characters/nim/frames/ in place.
The manifests and the Personality Prompts are written by hand and never touched.
"""

import math
import pathlib
import struct
import zlib

SIZE = 32
# The bottom row is where the Character's feet are: `place_sprite` hangs the art
# a whole height above the contact point, so art that stops short of row 31
# floats above the floor by however much it stopped short by.
GROUND = SIZE - 1
ROOT = pathlib.Path(__file__).resolve().parent.parent / "characters"

CLEAR = (0, 0, 0, 0)


# --------------------------------------------------------------------------
# The grid
# --------------------------------------------------------------------------


def blank():
    return [[CLEAR] * SIZE for _ in range(SIZE)]


def put(px, x, y, color):
    if 0 <= x < SIZE and 0 <= y < SIZE:
        px[y][x] = color


def rect(px, x0, y0, w, h, color):
    for y in range(y0, y0 + h):
        for x in range(x0, x0 + w):
            put(px, x, y, color)


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


# --------------------------------------------------------------------------
# Chip — faithful Win95
# --------------------------------------------------------------------------

# The sixteen colours a VGA text mode had, which is the whole palette Chip is
# allowed. Anything between two of them is dithered, never mixed.
BLACK = (0x00, 0x00, 0x00, 255)
NAVY = (0x00, 0x00, 0x80, 255)
DGREEN = (0x00, 0x80, 0x00, 255)
TEAL = (0x00, 0x80, 0x80, 255)
MAROON = (0x80, 0x00, 0x00, 255)
PURPLE = (0x80, 0x00, 0x80, 255)
OLIVE = (0x80, 0x80, 0x00, 255)
SILVER = (0xC0, 0xC0, 0xC0, 255)
GREY = (0x80, 0x80, 0x80, 255)
BLUE = (0x00, 0x00, 0xFF, 255)
GREEN = (0x00, 0xFF, 0x00, 255)
CYAN = (0x00, 0xFF, 0xFF, 255)
RED = (0xFF, 0x00, 0x00, 255)
MAGENTA = (0xFF, 0x00, 0xFF, 255)
YELLOW = (0xFF, 0xFF, 0x00, 255)
WHITE = (0xFF, 0xFF, 0xFF, 255)

VGA = {
    BLACK, NAVY, DGREEN, TEAL, MAROON, PURPLE, OLIVE, SILVER,
    GREY, BLUE, GREEN, CYAN, RED, MAGENTA, YELLOW, WHITE,
}


def dither(px, x0, y0, w, h, a, b):
    """Two colours on a checkerboard — the era's only way to shade."""
    for y in range(y0, y0 + h):
        for x in range(x0, x0 + w):
            put(px, x, y, a if (x + y) % 2 == 0 else b)


def bevel(px, x0, y0, w, h, fill, light, dark):
    """A raised Win95 control: lit top and left, shadowed bottom and right."""
    rect(px, x0, y0, w, h, fill)
    rect(px, x0, y0, w, 1, light)
    rect(px, x0, y0, 1, h, light)
    rect(px, x0, y0 + h - 1, w, 1, dark)
    rect(px, x0 + w - 1, y0, 1, h, dark)
    put(px, x0 + w - 1, y0, dark)
    put(px, x0, y0 + h - 1, dark)


def screen(px, top, eyes, mouth, asleep):
    """The CRT face: recessed, scanlined, and dark when Chip is asleep."""
    x0, y0, w, h = 10, top + 2, 12, 7
    rect(px, x0 - 1, y0 - 1, w + 2, h + 2, GREY)
    rect(px, x0 - 1, y0 + h, w + 2, 1, WHITE)
    rect(px, x0 + w, y0 - 1, 1, h + 2, WHITE)

    if asleep:
        # A dark screen with a screensaver crawling over it is what a 1995
        # machine looks like when nobody is at it.
        rect(px, x0, y0, w, h, BLACK)
        return
    # Scanlines: teal and dark-teal rows, which is dithering by row rather than
    # by pixel and reads as a phosphor screen.
    for row in range(h):
        rect(px, x0, y0 + row, w, 1, TEAL if row % 2 == 0 else DGREEN)

    if eyes == "shut":
        rect(px, 12, y0 + 3, 3, 1, GREEN)
        rect(px, 17, y0 + 3, 3, 1, GREEN)
    else:
        wide = eyes == "wide"
        for x in (12, 17):
            rect(px, x, y0 + 1, 3, 3 if wide else 2, GREEN)
            if wide:
                put(px, x + 1, y0 + 2, BLACK)
    if mouth:
        rect(px, 13, y0 + 5, 6, min(mouth, 2), GREEN)
    else:
        rect(px, 13, y0 + 5, 6, 1, DGREEN)


def chip(drop=0, eyes="open", mouth=0, stride=0, arms=0, bulb=RED, asleep=False, mark=None):
    """Chip, posed. `drop` compresses him toward the floor; the feet never move."""
    px = blank()
    head_top = 6 + drop
    body_top = 19 + drop
    body_bottom = 28

    # Antenna, above everything, with a bulb that changes colour rather than
    # position: a blinking light is the cheapest liveliness a box has. It
    # shortens rather than leaving the grid when Chip rises.
    bulb_top = max(0, head_top - 6)
    rect(px, 16, bulb_top + 2, 1, head_top - bulb_top - 2, GREY)
    rect(px, 15, bulb_top, 2, 2, bulb)

    bevel(px, 8, head_top, 16, 12, SILVER, WHITE, GREY)
    screen(px, head_top, eyes, mouth, asleep)
    rect(px, 14, head_top + 12, 4, body_top - head_top - 12, GREY)

    bevel(px, 6, body_top, 20, body_bottom - body_top + 1, SILVER, WHITE, GREY)
    # A shaded lower half, in the only way sixteen colours allow.
    dither(px, 7, body_bottom - 3, 18, 3, SILVER, GREY)
    rect(px, 8, body_top + 2, 3, 2, MAROON if asleep else RED)
    for row in range(2):
        rect(px, 14, body_top + 2 + row * 2, 8, 1, GREY)

    for x, side in ((3, -1), (26, 1)):
        rect(px, x, body_top + 2 + arms * side, 3, 3, GREY)
        rect(px, x, body_top + 2 + arms * side, 3, 1, SILVER)

    # Feet last: they are the only part that stands on the floor, so nothing
    # may be drawn over them.
    for x in (8 - stride, 18 + stride):
        rect(px, x, GROUND - 2, 6, 3, GREY)
        rect(px, x, GROUND - 2, 6, 1, SILVER)
        rect(px, x, GROUND, 6, 1, BLACK)

    if mark == "bang":
        rect(px, 26, 0, 2, 5, YELLOW)
        rect(px, 26, 6, 2, 2, YELLOW)
    elif mark is not None:  # a screensaver Z, at the offset given
        zx, zy = 12 + mark, head_top + 4
        rect(px, zx, zy, 5, 1, GREEN)
        rect(px, zx, zy + 4, 5, 1, GREEN)
        for i in range(3):
            put(px, zx + 3 - i, zy + 1 + i, GREEN)
    return px


def chip_animations():
    """Chip's eight Animations. Two or three frames each, held hard: a machine
    from 1995 snaps between poses rather than easing between them."""
    return {
        "idle": [chip(), chip(bulb=MAROON)],
        "walk": [
            chip(stride=2, arms=1),
            chip(drop=1, stride=0),
            chip(stride=2, arms=-1),
            chip(drop=1, stride=0, bulb=MAROON),
        ],
        "fall": [chip(drop=-2, arms=-2, eyes="wide", mouth=2), chip(drop=-1, arms=-2, eyes="wide", mouth=1)],
        "land": [chip(drop=4, eyes="shut", stride=3), chip(drop=1, stride=1)],
        "sit": [chip(drop=6, stride=3), chip(drop=6, stride=3, bulb=MAROON)],
        "sleep": [
            chip(drop=6, stride=3, eyes="shut", asleep=True, bulb=MAROON, mark=0),
            chip(drop=6, stride=3, eyes="shut", asleep=True, bulb=MAROON, mark=4),
        ],
        "react": [
            chip(drop=-1, eyes="wide", mouth=2, arms=-1, bulb=YELLOW, mark="bang"),
            chip(eyes="wide", mouth=1, bulb=YELLOW, mark="bang"),
        ],
        "talk": [chip(mouth=1), chip(mouth=2, bulb=MAROON), chip(mouth=0)],
    }


# --------------------------------------------------------------------------
# Nim — modern pixel art
# --------------------------------------------------------------------------

# A lit ramp rather than a palette of flat fills: every shade of Nim is one of
# these, chosen by which way the surface faces the light.
PLUM = [
    (0x2A, 0x1E, 0x3A, 255),
    (0x40, 0x2D, 0x57, 255),
    (0x5B, 0x40, 0x78, 255),
    (0x7A, 0x57, 0x9C, 255),
    (0x9A, 0x73, 0xBD, 255),
    (0xB8, 0x94, 0xD8, 255),
    (0xD6, 0xBA, 0xEE, 255),
    (0xEF, 0xE1, 0xFA, 255),
]
CREAM = [
    (0xC9, 0x9C, 0x74, 255),
    (0xE8, 0xC0, 0x95, 255),
    (0xF7, 0xDF, 0xBE, 255),
    (0xFF, 0xF3, 0xE0, 255),
]
EYE_WHITE = (0xFB, 0xF6, 0xFF, 255)
IRIS = [(0x18, 0x4E, 0x6B, 255), (0x2F, 0x8F, 0xB5, 255), (0x7A, 0xD8, 0xEE, 255)]
BLUSH = (0xE0, 0x8C, 0xA8, 255)
MOUTH = (0x3A, 0x22, 0x3C, 255)
SPARK = (0xFF, 0xF2, 0xA8, 255)
# The one translucent colour Nim has. Being under the hit-test threshold as
# well as under Nim, it is a shadow a click goes straight through.
SHADOW = (0x2A, 0x1E, 0x3A, 0x4C)

LIGHT = (-0.55, -0.84)


def shade(nx, ny, ramp, lift=0):
    """Which step of `ramp` a point on a blob's surface takes.

    The blob is lit as a sphere: how far the surface faces the light picks the
    step, and the band just inside the far edge takes a rim light, which is
    what stops a shaded ball from reading as a flat disc.
    """
    facing = nx * LIGHT[0] + ny * LIGHT[1]
    step = (facing + 1.0) / 2.0
    index = int(round(step * (len(ramp) - 1))) + lift
    if nx * nx + ny * ny > 0.62 and facing < -0.35:
        index += 2
    return ramp[max(0, min(len(ramp) - 1, index))]


def blob(px, cx, cy, rx, ry, ramp, lift=0):
    """A shaded ellipse. Nim is three of them and a pair of ears."""
    for y in range(int(cy - ry) - 1, int(cy + ry) + 2):
        for x in range(int(cx - rx) - 1, int(cx + rx) + 2):
            nx = (x + 0.5 - cx) / rx
            ny = (y + 0.5 - cy) / ry
            if nx * nx + ny * ny <= 1.0:
                put(px, x, y, shade(nx, ny, ramp, lift))


def ear(px, x, top, sway, flip, length=7):
    """An ear that trails the head: it is anchored where it meets the skull and
    leans further the nearer the tip, so a sway is a curve rather than a slide.
    A drooping ear is a short one."""
    for row in range(7 - length, 7):
        lean = int(round(sway * ((6 - row) / 6.0) ** 2 * 2.0))
        rx = x + flip * lean
        ramp = PLUM[6 - row // 3] if flip > 0 else PLUM[4 - row // 3]
        put(px, rx, top + row, ramp)
        put(px, rx + flip, top + row, PLUM[2])


def eyes(px, cy, open_amount, look=0):
    """Two large eyes with an iris ramp and a highlight. `open_amount` is 0 to
    1, and everything between is a frame of a blink."""
    for side in (-4, 4):
        cx = 16 + side
        if open_amount < 0.35:
            rect(px, cx - 2, cy + 1, 4, 1, PLUM[1])
            continue
        height = 3 if open_amount < 0.75 else 4
        rect(px, cx - 2, cy, 4, height, EYE_WHITE)
        rect(px, cx - 1 + look, cy + height - 3, 2, height - 1, IRIS[1])
        rect(px, cx - 1 + look, cy + height - 2, 2, 1, IRIS[0])
        put(px, cx - 1 + look, cy + height - 3, IRIS[2])


def nim(bob=0.0, squash=0.0, sway=0.0, open_amount=1.0, look=0, mouth=0, step=0.0, arms=0, ears=7, mark=None, shadow=True):
    """Nim, posed. Every argument is continuous, which is what buys the
    in-between frames: a walk is the same pose sampled eight times."""
    px = blank()
    # A contact shadow needs contact: the only Animation the Engine plays with
    # the sprite off the ground turns it off, or Nim falls with the floor
    # attached to its feet.
    if shadow:
        rect(px, 11, GROUND, 10, 1, SHADOW)
        rect(px, 12, GROUND - 1, 8, 1, SHADOW)

    body_cy = 23.0 + bob + squash
    head_cy = 14.5 + bob

    # Feet, under the body: a walk swings them along the same arc, half a cycle
    # apart, and a rest leaves them side by side.
    for side, phase in ((-1, 0.0), (1, math.pi)):
        swing = math.sin(step + phase)
        fx = 13 + (0 if side < 0 else 5) + int(round(swing * 2))
        fy = GROUND - 2 - max(0, int(round(math.sin(step + phase) * 1.5)))
        rect(px, fx, fy, 4, 2, PLUM[2 if swing < 0 else 3])
        rect(px, fx, fy, 4, 1, PLUM[4])

    blob(px, 16, body_cy, 8.5, 6.5 - squash, PLUM)
    blob(px, 16, body_cy + 1.0, 5.0, max(1.5, 4.0 - squash), CREAM)

    for side in (-1, 1):
        ax = 16 + side * 9
        rect(px, ax - 1, int(body_cy) - 1 + arms * side, 2, 4, PLUM[3 if side < 0 else 5])

    ear(px, 11, int(head_cy) - 11, sway, -1, ears)
    ear(px, 20, int(head_cy) - 11, sway, 1, ears)
    blob(px, 16, head_cy, 7.5, 6.5, PLUM, lift=1)
    eyes(px, int(head_cy) - 1, open_amount, look)
    put(px, 11, int(head_cy) + 2, BLUSH)
    put(px, 21, int(head_cy) + 2, BLUSH)
    if mouth:
        rect(px, 15, int(head_cy) + 3, 2, min(mouth, 3), MOUTH)
    else:
        put(px, 15, int(head_cy) + 3, MOUTH)
        put(px, 16, int(head_cy) + 3, MOUTH)

    if mark == "spark":
        for x, y in ((25, 4), (24, 5), (26, 5), (25, 6), (25, 3), (25, 7)):
            put(px, x, y, SPARK)
    elif mark is not None:
        zx, zy = 23, 5 - mark
        rect(px, zx, zy, 4, 1, PLUM[6])
        rect(px, zx, zy + 3, 4, 1, PLUM[6])
        for i in range(2):
            put(px, zx + 2 - i, zy + 1 + i, PLUM[6])
    return px


def nim_animations():
    """Nim's eight Animations. Twice the frames of Chip's everywhere, because
    the whole difference between them is that Nim eases and Chip snaps."""
    idle = []
    for i in range(6):
        phase = i / 6.0 * math.tau
        idle.append(
            nim(
                bob=math.sin(phase) * 0.9,
                sway=math.sin(phase - 0.6),
                open_amount=0.0 if i == 4 else 1.0,
            )
        )

    walk = []
    for i in range(8):
        phase = i / 8.0 * math.tau
        walk.append(
            nim(
                bob=abs(math.sin(phase)) * -1.2,
                sway=math.sin(phase - 1.2) * 0.9,
                step=phase,
                arms=1 if math.sin(phase) > 0 else -1,
            )
        )

    fall = [
        nim(bob=-1.5, sway=-1.0, arms=-2, mouth=2, shadow=False),
        nim(bob=-1.0, sway=1.0, arms=-2, mouth=3, shadow=False),
        nim(bob=-1.4, sway=-0.7, arms=-2, mouth=2, shadow=False),
    ]
    land = [
        nim(squash=3.0, bob=2.0, sway=1.0, open_amount=0.2, arms=1, ears=5),
        nim(squash=1.8, bob=1.0, sway=0.8, open_amount=0.5, ears=6),
        nim(squash=0.8, sway=0.5, open_amount=0.9),
        nim(squash=0.2, sway=0.2),
    ]
    sit = [
        nim(squash=1.4 + i * 0.2, bob=1.0, sway=0.4 - i * 0.2, look=i % 2, ears=6)
        for i in range(4)
    ]
    sleep = []
    for i in range(4):
        sleep.append(
            nim(
                squash=2.0 + math.sin(i / 4.0 * math.tau) * 0.4,
                bob=1.2,
                sway=0.2,
                open_amount=0.0,
                ears=4,
                mark=i,
            )
        )
    react = [
        nim(bob=-2.0, sway=-0.6, mouth=3, arms=-1, mark="spark"),
        nim(bob=-2.6, sway=0.6, mouth=2, arms=-1, mark="spark"),
        nim(bob=-1.4, sway=0.8, mouth=2, mark="spark"),
        nim(bob=-0.6, sway=0.4, mouth=1),
        nim(bob=0.0, sway=0.2, open_amount=0.8),
    ]
    talk = []
    for i in range(6):
        phase = i / 6.0 * math.tau
        talk.append(
            nim(
                bob=math.sin(phase) * 0.6,
                sway=math.sin(phase) * 0.5,
                mouth=(0, 2, 3, 2, 1, 0)[i],
                look=(0, 0, 1, 0, -1, 0)[i],
            )
        )

    return {
        "idle": idle,
        "walk": walk,
        "fall": fall,
        "land": land,
        "sit": sit,
        "sleep": sleep,
        "react": react,
        "talk": talk,
    }


# --------------------------------------------------------------------------


def colours(animations):
    return {px[y][x] for frames in animations.values() for px in frames for y in range(SIZE) for x in range(SIZE)}


def write(name, animations):
    out = ROOT / name / "frames"
    out.mkdir(parents=True, exist_ok=True)
    for existing in out.glob("*.png"):
        existing.unlink()
    for animation, frames in animations.items():
        for index, px in enumerate(frames):
            (out / f"{animation}-{index}.png").write_bytes(png(px))
    print(f"{name}: {sum(len(f) for f in animations.values())} frames, "
          f"{len(colours(animations) - {CLEAR})} colours")


def main():
    chip_art = chip_animations()
    nim_art = nim_animations()

    # The check the styles are held to. Chip is a sixteen-colour Character and
    # Nim is not, and a stray colour in either is the failure this file exists
    # to make impossible: two Characters that differ only in their palette are
    # the reskin #9 says is not good enough.
    used = colours(chip_art) - {CLEAR}
    assert used <= VGA, f"Chip drew outside the VGA sixteen: {used - VGA}"
    assert len(used) <= 15, f"Chip drew {len(used)} colours and transparency"
    assert len(colours(nim_art) - {CLEAR}) > 16, "Nim fits in a sixteen-colour palette"

    for name, art in (("chip", chip_art), ("nim", nim_art)):
        assert set(art) == {"idle", "walk", "fall", "land", "sit", "sleep", "react", "talk"}
        write(name, art)


if __name__ == "__main__":
    main()
