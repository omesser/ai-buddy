#!/usr/bin/env python3
"""Draw the two shipped Characters, BMO and Nim.

The two exist to prove the Character Package format against real variance, so
they are drawn by two different techniques rather than one technique twice:

  * **BMO** is hard-edged flat art — eight colours, flat fills, no
    anti-aliasing, and ordered dithering only where a shade between two of
    those eight is genuinely wanted. Few frames, held hard.
  * **Nim** is modern pixel art — a shaded ramp lit from the upper left, a
    palette four times the size, translucent contact shadow, and enough
    in-between frames that the motion reads as smooth rather than stepped.

Pure standard library, as `make-blip-character.py` is and for the same
reason: a build step that needs Pillow installed is a build step that stops
working. The PNG writer is copied from there rather than shared, because a
third file to import would cost more than the twelve lines it saves.

    python3 scripts/make-shipped-characters.py

It rewrites characters/bmo/frames/ and characters/nim/frames/ in place.
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
# BMO — the little living console
# --------------------------------------------------------------------------

# Eight flat colours and nothing between them. A console moulded from coloured
# plastic has no gradients in it, so anything wanting a shade between two of
# these is dithered rather than mixed — and rarely, because BMO is one flat
# shell rather than a lit box.
MINT = (0x7A, 0xD4, 0xBE, 255)
# The limbs, a step darker than the shell: in the source design BMO's arms and
# legs are moulded from darker plastic than the body, and limbs drawn in MINT
# read as part of the shell rather than hanging off it.
LIMB = (0x58, 0xA9, 0x96, 255)
DEEP = (0x3C, 0x8B, 0x7C, 255)
GLASS = (0xE9, 0xF6, 0xEC, 255)
INK = (0x1B, 0x2B, 0x33, 255)
RED = (0xD8, 0x3A, 0x3A, 255)
YELLOW = (0xF2, 0xC4, 0x3D, 255)
BLUE = (0x3F, 0x7C, 0xD8, 255)

PALETTE = {MINT, LIMB, DEEP, GLASS, INK, RED, YELLOW, BLUE}


def dither(px, x0, y0, w, h, a, b):
    """Two palette colours on a checkerboard — the only shade BMO is allowed."""
    for y in range(y0, y0 + h):
        for x in range(x0, x0 + w):
            put(px, x, y, a if (x + y) % 2 == 0 else b)


def rounded(px, x0, y0, w, h, fill, edge):
    """A rounded rectangle with a hard one-pixel rim. The rim is what keeps a
    mint shell legible on a pale desktop, where the fill alone washes out."""
    rect(px, x0, y0, w, h, fill)
    rect(px, x0, y0, w, 1, edge)
    rect(px, x0, y0 + h - 1, w, 1, edge)
    rect(px, x0, y0, 1, h, edge)
    rect(px, x0 + w - 1, y0, 1, h, edge)
    for x in (x0, x0 + w - 1):
        for y in (y0, y0 + h - 1):
            put(px, x, y, CLEAR)


def face(px, bx, top, eyes, mouth, dim):
    """The screen and the face inside it. This is the whole of BMO's
    expression: everything else only tilts and bobs around it. The face is
    sparse on purpose — two dark ovals set high and one shallow wide curve set
    low, and nothing else drawn at all."""
    rounded(px, bx + 2, top + 2, 14, 10, GLASS, DEEP)
    if dim:
        # Asleep the screen is turned down, not off — a difference one flat
        # colour cannot say and a checkerboard of two can.
        dither(px, bx + 3, top + 3, 12, 8, GLASS, MINT)

    if eyes == "shut":
        # Closed eyes are curves, or a sleeping BMO reads as a switched-off one.
        for x in (bx + 4, bx + 10):
            rect(px, x + 1, top + 5, 2, 1, INK)
            put(px, x, top + 6, INK)
            put(px, x + 3, top + 6, INK)
    elif eyes == "wide":
        # Startled eyes open upward and leave a clear row above the mouth. Grown
        # downward instead they meet it, and the whole face becomes one smudge.
        for x in (bx + 4, bx + 10):
            rect(px, x, top + 3, 3, 4, INK)
    else:
        for x in (bx + 5, bx + 11):
            rect(px, x, top + 4, 2, 3, INK)

    if mouth == "open":
        rect(px, bx + 7, top + 8, 4, 2, INK)
    elif mouth == "wide":
        # An open mouth is round, not a slab: four across with the corners off,
        # set low enough that it never touches the wide eyes above it.
        rect(px, bx + 7, top + 8, 4, 3, INK)
        for y in (top + 8, top + 10):
            put(px, bx + 7, y, CLEAR)
            put(px, bx + 10, y, CLEAR)
    elif mouth == "gasp":
        rect(px, bx + 8, top + 8, 2, 3, INK)
    elif mouth == "flat":
        rect(px, bx + 7, top + 9, 4, 1, INK)
    else:
        # The signature: six pixels wide and two tall, the ends a pixel above
        # the line. A deeper curve is a grin and a shorter one is a dot, and
        # BMO is neither.
        put(px, bx + 6, top + 8, INK)
        rect(px, bx + 7, top + 9, 4, 1, INK)
        put(px, bx + 11, top + 8, INK)


def button(px, x, y, colour):
    """Four across with the corners knocked off — the smallest circle this grid
    can draw. Three across leaves a plus, which is the D-pad's shape and would
    read as a second one."""
    rect(px, x, y + 1, 4, 2, colour)
    rect(px, x + 1, y, 2, 1, colour)
    rect(px, x + 1, y + 3, 2, 1, colour)


def panel(px, bx, top):
    """What makes it a console rather than a green box: a navy D-pad, a red and
    a blue button beside it, the yellow play triangle, and the cartridge slot.
    Every one of them stands clear of its neighbours — crowded, they merge into
    one coloured smudge at the size this is actually seen."""
    rect(px, bx + 2, top + 15, 5, 1, INK)
    rect(px, bx + 4, top + 13, 1, 5, INK)

    button(px, bx + 8, top + 13, RED)
    button(px, bx + 13, top + 13, BLUE)

    for row, width in enumerate((1, 2, 1)):
        rect(px, bx + 12, top + 18 + row, width, 1, YELLOW)

    rect(px, bx + 2, top + 19, 6, 2, INK)


def arm(px, x0, y, out, slope):
    """A long thin arm: two pixels thick, sloping over its length, with a small
    hand on the end. Long light limbs hung off a heavy shell are most of what
    separates BMO from a green box with stumps, so they reach well clear of the
    body rather than sitting flush against it."""
    for i in range(4):
        x = x0 + out * i
        put(px, x, y + slope * (i // 2), LIMB)
        put(px, x, y + slope * (i // 2) + 1, DEEP)
    for x in (x0 + out * 4, x0 + out * 5):
        rect(px, x, y + slope * 2, 1, 2, LIMB)
        put(px, x, y + slope * 2 + 2, DEEP)


def bmo(drop=0, eyes="open", mouth="smile", stride=0, lift=0, arms=0, swing=0, fold=False, dim=False):
    """BMO, posed. `drop` lowers the body and shortens the legs to meet it, so
    the feet stay on the bottom row however far down the body comes. `lift`
    takes one foot off that row, which is the only thing that is allowed to."""
    px = blank()
    bx = 7
    top = 3 + drop
    bottom = top + 23

    # A raised arm bends upward from the shoulder. Reusing the resting droop
    # for it would put the hands below the elbows with the arms overhead.
    slope = -1 if arms < 0 else 1
    for x0, out in ((bx - 1, -1), (bx + 18, 1)):
        arm(px, x0, top + 12 + arms + swing * out, out, slope)

    rounded(px, bx, top, 18, 24, MINT, DEEP)
    # The one shade on the shell, and the only place one is wanted: the underside
    # of a moulded case turns away from everything that lights it.
    dither(px, bx + 1, bottom - 2, 16, 2, MINT, DEEP)
    face(px, bx, top, eyes, mouth, dim)
    panel(px, bx, top)

    if fold:
        # Sitting, the legs are under BMO rather than beside it: one block
        # where two thin supports were.
        rect(px, 10, bottom + 1, 12, GROUND - bottom, LIMB)
    else:
        for side, x in ((-1, 11 - stride), (1, 19 + stride)):
            # A leg that swings through is a leg off the floor. Two pixels is
            # the whole of it at this size, and it is what separates a stride
            # from both feet sliding apart and back.
            sole = GROUND - (2 if lift == side else 0)
            rect(px, x, bottom + 1, 2, sole - bottom - 1, LIMB)
            rect(px, x, sole - 1, 2, 2, DEEP)
    return px


def bmo_animations():
    """BMO's nine Animations. Two to four frames each, cut fast: a handheld
    answers the instant a button is pressed, and the frame counts are where
    that reads. What changes between frames is mostly the face."""
    return {
        "idle": [bmo(), bmo(drop=1)],
        # A gait is a bob, not a lean: the body sits low with both feet planted
        # and rises as one leg swings under it. Four poses, none repeated, or
        # the cycle reads as the two it actually draws.
        "walk": [
            bmo(drop=1, stride=2, swing=2),
            bmo(stride=-1, lift=-1),
            bmo(drop=1, stride=2, swing=-2),
            bmo(stride=-1, lift=1),
        ],
        "fall": [
            bmo(drop=-1, arms=-6, eyes="wide", mouth="gasp", stride=2),
            bmo(arms=-5, eyes="wide", mouth="gasp", stride=1),
        ],
        "land": [bmo(drop=4, eyes="shut", mouth="wide", stride=2), bmo(drop=1)],
        "sit": [bmo(drop=4, fold=True), bmo(drop=4, fold=True, eyes="shut")],
        # Lower than a sit, arms down the sides: gripping a moving Perch, not
        # resting on a still one.
        "hold": [
            bmo(drop=5, fold=True, arms=1, eyes="wide"),
            bmo(drop=5, fold=True, arms=2, eyes="wide"),
        ],
        "sleep": [
            bmo(drop=4, fold=True, eyes="shut", mouth="flat", dim=True),
            bmo(drop=5, fold=True, eyes="shut", mouth="flat", dim=True),
        ],
        "react": [
            bmo(drop=-1, arms=-3, eyes="wide", mouth="gasp"),
            bmo(arms=-1, eyes="wide", mouth="wide"),
        ],
        "talk": [bmo(mouth="open"), bmo(drop=1, mouth="wide"), bmo(mouth="smile")],
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
    """Nim's nine Animations. Twice the frames of BMO's everywhere, because
    the whole difference between them is that Nim eases and BMO snaps."""
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
        "hold": [
            nim(squash=2.2 + i * 0.15, bob=1.5, sway=0.4 - i * 0.2, look=i % 2, ears=5, arms=1 if i % 2 == 0 else -1)
            for i in range(4)
        ],
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
    bmo_art = bmo_animations()
    nim_art = nim_animations()

    # The check the styles are held to. BMO is flat and hard-edged and Nim is
    # neither, and a stray colour in either is the failure this file exists to
    # make impossible: two Characters that differ only in their palette are the
    # reskin #9 says is not good enough.
    used = colours(bmo_art) - {CLEAR}
    assert used <= PALETTE, f"BMO drew outside its palette: {used - PALETTE}"
    assert all(colour[3] == 255 for colour in used), "BMO drew a soft edge"
    assert len(colours(nim_art) - {CLEAR}) > 16, "Nim fits in a sixteen-colour palette"

    for name, art in (("bmo", bmo_art), ("nim", nim_art)):
        assert set(art) == {
            "idle", "walk", "fall", "land", "sit", "sleep", "react", "talk", "hold"
        }
        write(name, art)


if __name__ == "__main__":
    main()
