#!/usr/bin/env python3
"""Cut the product logo to the macOS app icon grid.

The logo art is a full-bleed square, and nothing downstream rounds it: Tauri
hands the bundle icon to `NSApp.setApplicationIconImage` on a dev run and packs
it into the .icns for a bundled one, and neither path applies a mask. Shipped
raw, the Dock draws a hard-edged square that overhangs every neighbouring icon
by the margin they all leave. So the shape is baked in here — Apple's grid, an
824 body on a 1024 canvas, corners on a continuous-curvature squircle.

Pure standard library, for the reason make-shipped-characters.py gives: a build
step that needs Pillow installed is a build step that stops working.

    python3 scripts/make-app-icon.py

It rewrites src-tauri/icons/icon.png and src-tauri/icons/icon.ico from
branding/logo-art/logo-512.png. The tray mark is a different shape for a
different surface and is never touched.
"""

import math
import pathlib
import struct
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "branding" / "logo-art" / "logo-512.png"
ICON_PNG = ROOT / "src-tauri" / "icons" / "icon.png"
ICON_ICO = ROOT / "src-tauri" / "icons" / "icon.ico"

CANVAS = 1024
# Apple's macOS icon grid. The 200px of transparent margin is not padding we
# could tighten to taste: the Dock sizes every icon by the canvas and expects
# the artwork to stop here, so a body drawn any larger sits proud of its row.
BODY = 824
# The art is composed for a full-bleed square, so dropping it whole into the
# body would shrink the head by the margin the grid takes — in a Dock row it
# then reads a size smaller than every neighbour. Scaling past the body and
# cropping back to it keeps the head at the weight the neighbours carry, and
# what the crop discards is uniform background. Past about 1.3 the collar
# starts meeting the edge.
ZOOM = 1.20
# The squircle as a superellipse, |x/a|^n + |y/a|^n = 1. n = 5 is the standard
# approximation of the shape macOS uses — flatter sides and a longer corner
# sweep than the circular arc a plain rounded rectangle would give, which is
# the whole difference between "rounded" and "looks native".
SQUIRCLE_N = 5.0
# Vertical supersampling of the mask edge. Horizontal coverage is already
# exact (the boundary is solved per sub-row), so four sub-rows is enough to
# keep the corners smooth at 1024 and clean once downscaled to 16.
SUB_ROWS = 4

# What the existing icon.ico carries, kept as-is. tauri-build compiles this
# into a Windows Resource whether or not Windows is a bundle target, and #247
# is the note that nothing regenerated it; this script is that something.
ICO_SIZES = (16, 24, 32, 48, 64, 256)

LANCZOS_A = 3


# --------------------------------------------------------------------------
# PNG
# --------------------------------------------------------------------------


def read_png(path):
    """Decode a non-interlaced 8-bit PNG to (width, height, RGBA bytearray)."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")

    width = height = channels = None
    idat = bytearray()
    pos = 8
    while pos < len(data):
        length, kind = struct.unpack(">I4s", data[pos : pos + 8])
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(
                ">IIBBBBB", body
            )
            if depth != 8 or interlace or colour not in (2, 6):
                raise ValueError(f"{path}: only 8-bit RGB/RGBA, non-interlaced")
            channels = 3 if colour == 2 else 4
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break

    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    out = bytearray(width * height * 4)
    prior = bytearray(stride)
    pos = 0
    for y in range(height):
        filt = raw[pos]
        line = bytearray(raw[pos + 1 : pos + 1 + stride])
        pos += 1 + stride
        unfilter(filt, line, prior, channels)
        row = y * width * 4
        for x in range(width):
            src = x * channels
            dst = row + x * 4
            out[dst : dst + 3] = line[src : src + 3]
            out[dst + 3] = line[src + 3] if channels == 4 else 255
        prior = line
    return width, height, out


def unfilter(filt, line, prior, bpp):
    if filt == 0:
        return
    if filt == 1:
        for i in range(bpp, len(line)):
            line[i] = (line[i] + line[i - bpp]) & 0xFF
    elif filt == 2:
        for i in range(len(line)):
            line[i] = (line[i] + prior[i]) & 0xFF
    elif filt == 3:
        for i in range(len(line)):
            left = line[i - bpp] if i >= bpp else 0
            line[i] = (line[i] + ((left + prior[i]) >> 1)) & 0xFF
    elif filt == 4:
        for i in range(len(line)):
            left = line[i - bpp] if i >= bpp else 0
            up_left = prior[i - bpp] if i >= bpp else 0
            line[i] = (line[i] + paeth(left, prior[i], up_left)) & 0xFF
    else:
        raise ValueError(f"unknown PNG filter {filt}")


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def write_png(path, width, height, rgba):
    path.write_bytes(encode_png(width, height, rgba))


def encode_png(width, height, rgba):
    # Paeth on every scanline rather than the unfiltered rows a first cut
    # writes. The art is a smooth gradient, so predicting each byte from its
    # neighbours is what makes it compress at all: 384 KB against 525 KB,
    # which is also the difference between clearing pre-commit's large-file
    # threshold and tripping it. Matches what an adaptive encoder picks here.
    stride = width * 4
    lines = []
    prior = bytes(stride)
    for y in range(height):
        line = bytes(rgba[y * stride : (y + 1) * stride])
        encoded = bytearray(stride)
        for i in range(stride):
            left = line[i - 4] if i >= 4 else 0
            up_left = prior[i - 4] if i >= 4 else 0
            encoded[i] = (line[i] - paeth(left, prior[i], up_left)) & 0xFF
        lines.append(b"\x04" + bytes(encoded))
        prior = line
    raw = b"".join(lines)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(
        b"IDAT", zlib.compress(raw, 9)
    ) + chunk(b"IEND", b"")


def chunk(kind, body):
    return (
        struct.pack(">I", len(body))
        + kind
        + body
        + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
    )


# --------------------------------------------------------------------------
# Resampling
# --------------------------------------------------------------------------


def lanczos_weights(src_len, dst_len):
    """Per-output-pixel taps, computed once and reused for every row/column."""
    scale = dst_len / src_len
    support = LANCZOS_A / min(scale, 1.0)
    taps = []
    for i in range(dst_len):
        centre = (i + 0.5) / scale - 0.5
        first = max(0, math.ceil(centre - support))
        last = min(src_len - 1, math.floor(centre + support))
        row = []
        total = 0.0
        for j in range(first, last + 1):
            w = lanczos((j - centre) * min(scale, 1.0))
            if w:
                row.append((j, w))
                total += w
        taps.append([(j, w / total) for j, w in row])
    return taps


def lanczos(x):
    if x == 0:
        return 1.0
    if abs(x) >= LANCZOS_A:
        return 0.0
    px = math.pi * x
    return LANCZOS_A * math.sin(px) * math.sin(px / LANCZOS_A) / (px * px)


def resample(width, height, rgba, dst_w, dst_h):
    """Lanczos resample, on premultiplied alpha so edges pull no dark fringe."""
    src = [0.0] * (width * height * 4)
    for i in range(0, len(rgba), 4):
        a = rgba[i + 3] / 255.0
        src[i] = rgba[i] * a
        src[i + 1] = rgba[i + 1] * a
        src[i + 2] = rgba[i + 2] * a
        src[i + 3] = rgba[i + 3]

    horizontal = [0.0] * (dst_w * height * 4)
    taps = lanczos_weights(width, dst_w)
    for y in range(height):
        row = y * width * 4
        out = y * dst_w * 4
        for x, row_taps in enumerate(taps):
            acc = [0.0, 0.0, 0.0, 0.0]
            for j, w in row_taps:
                p = row + j * 4
                acc[0] += src[p] * w
                acc[1] += src[p + 1] * w
                acc[2] += src[p + 2] * w
                acc[3] += src[p + 3] * w
            horizontal[out + x * 4 : out + x * 4 + 4] = acc

    out_px = bytearray(dst_w * dst_h * 4)
    taps = lanczos_weights(height, dst_h)
    for y, col_taps in enumerate(taps):
        dst = y * dst_w * 4
        for x in range(dst_w):
            acc = [0.0, 0.0, 0.0, 0.0]
            for j, w in col_taps:
                p = (j * dst_w + x) * 4
                acc[0] += horizontal[p] * w
                acc[1] += horizontal[p + 1] * w
                acc[2] += horizontal[p + 2] * w
                acc[3] += horizontal[p + 3] * w
            alpha = clamp(acc[3])
            scale = 255.0 / alpha if alpha else 0.0
            i = dst + x * 4
            out_px[i] = round(clamp(acc[0] * scale))
            out_px[i + 1] = round(clamp(acc[1] * scale))
            out_px[i + 2] = round(clamp(acc[2] * scale))
            out_px[i + 3] = round(alpha)
    return out_px


def clamp(v):
    return 0.0 if v < 0 else (255.0 if v > 255 else v)


# --------------------------------------------------------------------------
# The squircle
# --------------------------------------------------------------------------


def squircle_coverage(size):
    """Per-pixel coverage of the superellipse inscribed in a size×size square.

    Solved rather than sampled across the row: within a sub-row the boundary
    crosses at one x per side, so the pixels it cuts take their exact fraction
    and everything inside is full. Only the sub-rows approximate.
    """
    radius = size / 2.0
    coverage = [0.0] * (size * size)
    weight = 1.0 / SUB_ROWS
    for y in range(size):
        row = y * size
        for sub in range(SUB_ROWS):
            dy = abs((y + (sub + 0.5) / SUB_ROWS) - radius) / radius
            if dy >= 1.0:
                continue
            half = radius * (1.0 - dy**SQUIRCLE_N) ** (1.0 / SQUIRCLE_N)
            left, right = radius - half, radius + half
            for x in range(math.floor(left), math.ceil(right)):
                span = min(x + 1.0, right) - max(float(x), left)
                if span > 0:
                    coverage[row + x] += span * weight
    return coverage


# --------------------------------------------------------------------------


def main():
    width, height, art = read_png(SOURCE)
    filled = round(BODY * ZOOM)
    body = crop_centre(filled, resample(width, height, art, filled, filled), BODY)

    coverage = squircle_coverage(BODY)
    for i, c in enumerate(coverage):
        body[i * 4 + 3] = round(body[i * 4 + 3] * c)

    canvas = bytearray(CANVAS * CANVAS * 4)
    offset = (CANVAS - BODY) // 2
    for y in range(BODY):
        src = y * BODY * 4
        dst = ((y + offset) * CANVAS + offset) * 4
        canvas[dst : dst + BODY * 4] = body[src : src + BODY * 4]

    write_png(ICON_PNG, CANVAS, CANVAS, canvas)
    write_ico(ICON_ICO, canvas)
    print(f"{ICON_PNG.relative_to(ROOT)}: {CANVAS}×{CANVAS}, {BODY}px body")
    print(f"{ICON_ICO.relative_to(ROOT)}: {', '.join(str(s) for s in ICO_SIZES)}")


def crop_centre(size, rgba, want):
    offset = (size - want) // 2
    return bytearray(
        b"".join(
            bytes(rgba[((y + offset) * size + offset) * 4 :][: want * 4])
            for y in range(want)
        )
    )


def write_ico(path, canvas):
    """PNG-compressed entries, the form the icon this replaces already used."""
    images = [
        encode_png(size, size, resample(CANVAS, CANVAS, canvas, size, size))
        for size in ICO_SIZES
    ]
    offset = 6 + 16 * len(images)
    directory = bytearray()
    for size, image in zip(ICO_SIZES, images):
        directory += struct.pack(
            "<BBBBHHII", size & 0xFF, size & 0xFF, 0, 0, 1, 32, len(image), offset
        )
        offset += len(image)
    path.write_bytes(
        struct.pack("<HHH", 0, 1, len(images)) + bytes(directory) + b"".join(images)
    )


if __name__ == "__main__":
    main()
