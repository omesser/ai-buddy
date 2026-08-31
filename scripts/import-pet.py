#!/usr/bin/env python3
"""Import a desktop pet from a standardized ecosystem into a Character Package.

A one-time authoring tool, not a build step: it translates a pet once and the
output is ours to review and hand-tune. Unlike the make-*-characters scripts
this one needs Pillow — the petscodex ecosystem ships webp sprite sheets, and
decoding webp rules out the standard library. It is the one script allowed to.

    python3 scripts/import-pet.py ~/.codex/pets/cat --format petscodex -o characters/cat
    python3 scripts/import-pet.py --self-test

The importer prints the pet's license and refuses a silently-unknown one
without --accept-license. Success is declared only after the output passes
`character::load`, through the core crate's `validate` example.
"""

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import zipfile

REPO = pathlib.Path(__file__).resolve().parent.parent

# The renderer's scale is an integer 1..=4, so it can only enlarge; art whose
# stand height overshoots the shimeji band (~100-130 logical px, the height
# BMO stands at) is resampled down at import time instead.
TARGET_STAND = 120
STAND_BAND = (100, 130)
MAX_FPS = 60

# petscodex (petdex) sheets are a fixed grid of 8 columns, 192x208 cells, in
# two published layouts; integer-scaled variants are accepted. Mirrors
# petdex's src/lib/sprite-atlas.ts.
ATLAS_COLUMNS = 8
ATLAS_CELL = (192, 208)
ATLAS_LAYOUTS = ((1, 9), (2, 11))

# Row semantics from petdex's src/lib/pet-states.ts: (row, frames, duration
# ms). Rows 9-10 of the v2 layout carry no semantics in petdex's own code
# yet, so the importer reads only these nine.
PETSCODEX_ROWS = {
    "idle": (0, 6, 1100),
    "running-right": (1, 8, 1060),
    "running-left": (2, 8, 1060),
    "waving": (3, 4, 700),
    "jumping": (4, 5, 840),
    "failed": (5, 8, 1220),
    "waiting": (6, 6, 1010),
    "running": (7, 6, 820),
    "review": (8, 6, 1030),
}

# The Required Animation Set, from petdex rows: (animation, source row,
# frame indices or None for all, loop, variant_of). Jumping's five frames
# read anticipation, lift, peak, descent, settle — fall takes peak/descent,
# land descent/settle, hold the lift pair. Walk's entry is a placeholder:
# read_petscodex picks the row (see its doc — the drawn facing contradicts
# petdex's row labels). The unchosen walk row and row 7 (running in place)
# go unused; sleep is synthesized because petdex has no sleep row.
PETSCODEX_MAP = (
    ("idle", "idle", None, "forever", None),
    ("waiting", "waiting", None, "forever", "idle"),
    ("walk", "running-left", None, "forever", None),
    ("talk", "waving", None, "forever", None),
    ("fall", "jumping", (2, 3), "forever", None),
    ("land", "jumping", (3, 4), "once", None),
    ("hold", "jumping", (1, 2), "forever", None),
    ("react", "failed", None, "once", None),
    ("sit", "review", None, "forever", None),
)

# Row number to (frame count, loop duration), for --map overrides that name
# rows by the number a person counts on the sheet.
PETSCODEX_ROW_TABLE = {
    row: (count, duration) for row, count, duration in PETSCODEX_ROWS.values()
}


def parse_mapping(text):
    """One --map value, `animation=row[:i,j,...]`, into (animation, row,
    indices or None). petdex's row semantics are labels each generated pet
    interprets loosely — one pet's "jumping" row is a sword lunge — so the
    reviewer remaps animations to the rows this pet actually drew."""
    animation, eq, source = text.partition("=")
    row_text, _, index_text = source.partition(":")
    allowed = {name for name, *_ in PETSCODEX_MAP} | {"sleep"}
    if not eq or animation not in allowed:
        sys.exit(f"--map {text}: the animation is one of {', '.join(sorted(allowed))}")
    if not row_text.isdigit() or int(row_text) not in PETSCODEX_ROW_TABLE:
        sys.exit(f"--map {text}: the source is a sheet row, 0-8")
    row = int(row_text)
    indices = None
    if index_text:
        try:
            indices = tuple(int(i) for i in index_text.split(","))
        except ValueError:
            sys.exit(f"--map {text}: frame indices are integers, comma-separated")
        count = PETSCODEX_ROW_TABLE[row][0]
        if any(i < 0 or i >= count for i in indices):
            sys.exit(f"--map {text}: row {row} declares frames 0-{count - 1}")
    return animation, row, indices


# Shimeji-ee pose durations count scheduler ticks, ~40ms each.
SHIMEJI_TICK_MS = 40

# Shimeji-ee action names to the Required Animation Set — the BMO (#96)
# mapping decisions, recorded as this adapter's defaults. First declared
# candidate wins; packs rarely have a talk, so talk falls back to Stand,
# react to the pinched wriggle, and sleep to the synthesized idle breath.
SHIMEJI_MAP = (
    ("idle", ("Stand",), "forever"),
    ("walk", ("Walk",), "forever"),
    ("fall", ("Falling", "Fall"), "forever"),
    ("land", ("Bouncing", "Bounce", "Landing"), "once"),
    ("sit", ("Sit", "SitDown"), "forever"),
    ("sleep", ("Sleep", "LieDown", "Lie", "Sprawl"), "forever"),
    ("hold", ("Pinched", "Grabbed", "Dragged"), "forever"),
    ("react", ("Tripping", "Trip", "Pinched"), "once"),
    ("talk", ("Wave", "Greet", "Hello", "Stand"), "forever"),
    ("climb", ("ClimbWall", "Climb"), "forever"),
)

# Many distributed packs (shimejishop's among them) are bare shime PNGs that
# drop into Shimeji-ee's standard conf. When a pack carries no actions.xml of
# its own, these pose sequences — the standard conf's, for exactly the
# actions SHIMEJI_MAP wants — stand in for it.
SHIMEJI_DEFAULT_ACTIONS = {
    "Stand": [("shime1.png", "250")],
    "Walk": [("shime1.png", "6"), ("shime2.png", "6"),
             ("shime1.png", "6"), ("shime3.png", "6")],
    "Falling": [("shime4.png", "250")],
    "Bouncing": [("shime18.png", "4"), ("shime19.png", "4")],
    "Sit": [("shime11.png", "250")],
    "Sprawl": [("shime21.png", "250")],
    "Pinched": [("shime9.png", "5"), ("shime7.png", "5"), ("shime5.png", "5"),
                ("shime1.png", "5"), ("shime6.png", "5"), ("shime8.png", "5"),
                ("shime10.png", "5")],
    "Tripping": [("shime19.png", "8"), ("shime18.png", "4"),
                 ("shime20.png", "4"), ("shime20.png", "10"),
                 ("shime19.png", "4")],
    # One climb cycle; the standard conf repeats it four times with holds.
    "ClimbWall": [("shime14.png", "16"), ("shime12.png", "4"),
                  ("shime13.png", "4"), ("shime13.png", "16"),
                  ("shime12.png", "4"), ("shime14.png", "4")],
}

# A starter life — every shipped character declares behaviors, and these name
# only required animations, so any import can carry them until hand-tuning.
STARTER_BEHAVIORS = """
# A starter life, copied from BMO's — tune it to the pet.
[behaviors.walk]
play = ["walk"]
weight = 1
when = "idle over 30s"

[behaviors.patrol]
play = ["walk", "idle", "walk"]
then = "walk"
weight = 3
when = "idle over 2m"

[behaviors.fidget]
play = ["idle"]
weight = 2
when = "idle over 10s"

[behaviors.report]
play = ["talk", "idle", "talk"]
weight = 3
when = "idle under 1m"

[behaviors.greet]
play = ["talk"]
then = "patrol"
weight = 4
when = "idle under 10s"
"""


def detect_atlas(width, height):
    """Identify a petscodex atlas from sheet dimensions: (version, rows,
    cell width, cell height), or None when no layout fits."""
    for version, rows in ATLAS_LAYOUTS:
        if width % ATLAS_COLUMNS or height % rows:
            continue
        canonical_w = ATLAS_COLUMNS * ATLAS_CELL[0]
        canonical_h = rows * ATLAS_CELL[1]
        if width * canonical_h != height * canonical_w:
            continue
        return version, rows, width // ATLAS_COLUMNS, height // rows
    return None


def fps_for(frames, duration_ms):
    """Map a row's frame count and loop duration to a manifest fps."""
    return max(1, min(MAX_FPS, round(frames * 1000 / duration_ms)))


def animation_offset(union, canvas):
    """The uniform shift that centers an animation's union bbox horizontally
    and lands its collective baseline on the canvas bottom — the placement
    for a `registered` spec, whose frames' relative positions are meaningful
    (the synthesized sleep and its one-pixel breath)."""
    left, _, right, bottom = union
    width, height = canvas
    return (width - (right - left)) // 2 - left, height - bottom


def solid_mask(frame):
    """The pixels the loader itself will count as drawn: alpha at or above
    the hit-test threshold (`character::ALPHA_THRESHOLD` is 128). Alignment
    measures these rather than the anti-aliased halo."""
    return frame.getchannel("A").point(lambda v: 255 if v >= 128 else 0)


def x_offsets(frames):
    """Per-frame horizontal shifts registering every frame to the first, by
    maximum overlap of solid pixels. Generated sheets place the character a
    few pixels differently in every cell, which plays back as sideways
    drift; bbox or centroid centering would instead swing the body whenever
    a limb extends, so overlap does the registering."""
    from PIL import ImageChops

    anchor = solid_mask(frames[0])
    width, height = anchor.size
    reach = max(8, width // 4)

    def registered(mask):
        def overlap(shift):
            slid = mask.crop((shift, 0, shift + width, height))
            return ImageChops.darker(anchor, slid).histogram()[255]

        # The best shift, nearest to zero on a tie; drawing at its negation
        # moves this frame's art onto the anchor's.
        return -max(range(-reach, reach + 1), key=lambda s: (overlap(s), -abs(s)))

    return [0] + [registered(solid_mask(frame)) for frame in frames[1:]]


def import_scale(stand_height):
    """(resample factor, manifest scale) landing the sprite in the shimeji
    band. Art already in the band ships untouched at scale 1; taller art is
    resampled down; small pixel art keeps its pixels and scales up."""
    low, high = STAND_BAND
    if stand_height > high:
        return TARGET_STAND / stand_height, 1
    if stand_height < low:
        return 1.0, max(1, min(4, round(TARGET_STAND / stand_height)))
    return 1.0, 1


def render_mode(color_count, partial_alpha_fraction):
    """Drawn art anti-aliases its edges and spends colors freely; true pixel
    art has hard alpha and a small palette (ADR-0006). The bounds are loose
    on purpose: pixel art keeps translucency to a deliberate shadow (Nim's
    worst frame is 1.5% of its pixels) and a palette within an eight-bit
    indexed image's 256, while anti-aliased edges alone blow past both
    (BMO's worst frame is 3.9%, Cat's 12.7%)."""
    if partial_alpha_fraction > 0.02 or color_count > 256:
        return "smooth"
    return "pixelated"


def measure(frame):
    """The render_mode inputs, off one representative frame."""
    colors = frame.convert("RGB").getcolors(maxcolors=4096)
    color_count = len(colors) if colors else 4097
    histogram = frame.getchannel("A").histogram()
    partial = sum(histogram[1:255])
    total = sum(histogram)
    return color_count, partial / total if total else 0.0


def union_bbox(frames):
    """The bbox holding every frame's opaque pixels, or None when blank."""
    boxes = [box for box in (f.getchannel("A").getbbox() for f in frames) if box]
    if not boxes:
        return None
    return (
        min(box[0] for box in boxes),
        min(box[1] for box in boxes),
        max(box[2] for box in boxes),
        max(box[3] for box in boxes),
    )


def stillest(frames):
    """The frame differing least from the rest — the pose the animation
    keeps returning to."""
    from PIL import ImageChops

    def restlessness(i):
        return sum(
            value * count
            for other in frames
            for value, count in enumerate(
                ImageChops.difference(frames[i], other).convert("L").histogram()
            )
        )

    return frames[min(range(len(frames)), key=restlessness)]


def lifted(frame):
    """The frame shifted up one pixel — the breath a two-frame sleep needs."""
    from PIL import Image

    breath = Image.new("RGBA", frame.size, (0, 0, 0, 0))
    breath.paste(frame, (0, -1))
    return breath


def synthesized_sleep(idle_frames):
    """A sleep for sources that have none: idle's stillest frame, twice, the
    second lifted a pixel — BMO's breath, made a rule. `registered` keeps
    the per-frame grounding pass from flattening that pixel."""
    still = stillest(idle_frames)
    return {
        "frames": [still, lifted(still)],
        "fps": 1,
        "loop": "forever",
        "variant_of": None,
        "registered": True,
    }


def manifest_text(name, mode, scale, header, animations):
    """The Character Manifest, provenance as comments — the loader's key set
    is closed, so provenance lives in `#` lines like BMO's."""
    lines = [f"# {line}".rstrip() for line in header]
    lines += ["", f'name = "{name}"', f'render_mode = "{mode}"', f"scale = {scale}"]
    for animation, spec in animations.items():
        lines += ["", f"[animations.{animation}]"]
        # "order" repeats written frames the way the source sequenced them
        # (a shimeji walk is 1,2,1,3); a repeated path costs no pixel budget.
        order = spec.get("order") or range(len(spec["frames"]))
        frames = ", ".join(f'"frames/{animation}-{i}.png"' for i in order)
        lines.append(f"frames = [{frames}]")
        lines.append(f"fps = {spec['fps']}")
        if spec["loop"] == "once":
            lines.append('loop = "once"')
        if spec["variant_of"]:
            lines.append(f'variant_of = "{spec["variant_of"]}"')
    lines.append(STARTER_BEHAVIORS)
    return "\n".join(lines).rstrip() + "\n"


def read_petscodex(source, walk_row=2, mirror_walk=False, overrides=None):
    """A petscodex pet directory (`npx petscodex install <id>` lands one at
    ~/.codex/pets/<id>/) into the importer's common shape.

    `walk_row` picks the walk art. Petdex's semantics say row 1 heads right,
    but the rows render as drawn and every pet sampled so far (cat, labubu,
    tiga, hachiware) draws row 1 heading left — so row 2 is the default. No
    code can see facing, and row quality varies per pet (hachiware's row 2
    mixes camera angles and only its row 1 walks cleanly), so a human
    eyeballs the output and re-runs with --walk-row 1 and, when neither row
    heads right, --mirror-walk."""
    from PIL import Image, ImageOps

    pet = json.loads((source / "pet.json").read_text())
    sheet = Image.open(source / pet["spritesheetPath"]).convert("RGBA")
    atlas = detect_atlas(*sheet.size)
    if atlas is None:
        sys.exit(f"{sheet.size[0]}x{sheet.size[1]} matches no petscodex atlas layout")
    version, rows, cell_w, cell_h = atlas
    if rows > 9:
        print("atlas v2: rows 9-10 carry no petdex semantics yet, ignoring them")

    def row_frames(row, count):
        return [
            sheet.crop((c * cell_w, row * cell_h, (c + 1) * cell_w, (row + 1) * cell_h))
            for c in range(count)
        ]

    overrides = overrides or {}
    walk_state = "running-right" if walk_row == 1 else "running-left"
    animations = {}
    for animation, state, indices, loop, variant_of in PETSCODEX_MAP:
        if animation == "walk":
            state = walk_state
        row, count, duration = PETSCODEX_ROWS[state]
        if animation in overrides:
            row, indices = overrides[animation]
            count, duration = PETSCODEX_ROW_TABLE[row]
        frames = row_frames(row, count)
        if indices:
            frames = [frames[i] for i in indices]
        if animation == "walk" and mirror_walk:
            frames = [ImageOps.mirror(frame) for frame in frames]
        animations[animation] = {
            "frames": frames,
            "fps": fps_for(count, duration),
            "loop": loop,
            "variant_of": variant_of,
        }

    if "sleep" in overrides:
        row, indices = overrides["sleep"]
        count, duration = PETSCODEX_ROW_TABLE[row]
        frames = row_frames(row, count)
        animations["sleep"] = {
            "frames": [frames[i] for i in indices] if indices else frames,
            "fps": 1,
            "loop": "forever",
            "variant_of": None,
        }
    else:
        animations["sleep"] = synthesized_sleep(animations["idle"]["frames"])

    mirrored = ", mirrored to head right" if mirror_walk else ""
    header = [
        f"{pet['displayName']} imported from petscodex (petdex) by scripts/import-pet.py.",
        f"Source pet id: {pet['id']} — https://petscodex.com/pets/{pet['id']}",
        f"Mapping: idle<-row 0, walk<-{walk_state} row {walk_row}{mirrored}",
        "(the row drawn heading right), talk<-waving row 3,",
        "fall/land/hold<-jumping row 4, react<-failed row 5, waiting<-row 6",
        "(idle variant), sit<-review row 8; sleep is idle's stillest frame",
        "with a one-pixel breath. The other walk row (the engine mirrors",
        "instead) and row 7 (running in place) go unused.",
    ]
    if overrides:
        remapped = ", ".join(
            f"{animation}<-row {row}"
            + (f"[{','.join(map(str, indices))}]" if indices else "")
            for animation, (row, indices) in sorted(overrides.items())
        )
        header.append(f"Remapped for this pet's art (--map): {remapped}.")
    license_line = pet.get("pet_license") or pet.get("license")
    return {
        "name": pet["displayName"],
        "personality": pet.get("description", ""),
        "license": license_line,
        "header": header,
        "animations": animations,
    }


def read_shimeji(source, name=None):
    """A Shimeji-ee pack into the importer's common shape. Frames are
    per-pose PNGs (shime1.png…, at the root or under img/); pose semantics
    come from actions.xml, whose Action elements name Pose sequences — a
    richer source than frame numbering alone. Shimeji art walks left, so
    every frame is mirrored to head right, the engine's default (#96)."""
    import xml.etree.ElementTree as ET

    from PIL import Image, ImageOps

    frame_root = next(source.rglob("shime1.png"), None)
    if frame_root is None:
        sys.exit(f"{source}: no shime1.png anywhere — not a shimeji pack")
    frame_root = frame_root.parent
    actions_file = next(
        (p for pattern in ("actions.xml", "動作.xml") for p in source.rglob(pattern)),
        None,
    )
    if actions_file is None:
        print("no actions.xml in the pack; assuming Shimeji-ee's standard conf")
        actions = dict(SHIMEJI_DEFAULT_ACTIONS)
    else:
        # Tags are namespaced ({http://www.group-finity.com/Mascot}Action);
        # matching on the local name keeps every dialect readable. Sequence
        # actions compose other actions and carry no poses of their own.
        def local(tag):
            return tag.rsplit("}", 1)[-1]

        actions = {}
        for element in ET.parse(actions_file).iter():
            if local(element.tag) != "Action":
                continue
            poses = [
                (pose.get("Image").lstrip("/"), pose.get("Duration"))
                for pose in element.iter()
                if local(pose.tag) == "Pose" and pose.get("Image")
            ]
            if poses:
                actions.setdefault(element.get("Name"), poses)

    mirrored = {}

    def frame(image):
        if image not in mirrored:
            mirrored[image] = ImageOps.mirror(
                Image.open(frame_root / image).convert("RGBA")
            )
        return mirrored[image]

    animations = {}
    chosen = []
    missing = []
    for animation, candidates, loop in SHIMEJI_MAP:
        action = next((c for c in candidates if c in actions), None)
        if action is None:
            if animation not in ("sleep", "climb"):
                missing.append(f"{animation} (looked for {', '.join(candidates)})")
            continue
        poses = actions[action]
        # A pose sequence repeats images (walk is 1,2,1,3); write each
        # distinct frame once and let the manifest repeat the path, the way
        # BMO's hand-built manifest does.
        unique = list(dict.fromkeys(image for image, _ in poses))
        ticks = [int(duration) for _, duration in poses if duration]
        fps = (
            fps_for(len(poses), sum(ticks) * SHIMEJI_TICK_MS)
            if len(ticks) == len(poses)
            else 8
        )
        animations[animation] = {
            "frames": [frame(image) for image in unique],
            "order": [unique.index(image) for image, _ in poses],
            "fps": fps,
            "loop": loop,
            "variant_of": None,
        }
        chosen.append(f"{animation}<-{action}")
    if missing:
        where = actions_file.name if actions_file else "the standard conf"
        sys.exit(f"{where} declares no usable " + "; no ".join(missing))

    if "sleep" not in animations:
        animations["sleep"] = synthesized_sleep(animations["idle"]["frames"])
        chosen.append("sleep<-synthesized from idle")
    print("mapped:", ", ".join(chosen))

    name = name or source.name
    header = [
        f"{name} imported from a Shimeji-ee pack by scripts/import-pet.py.",
        "Every frame is mirrored to head right: shimeji art walks left, and",
        "the engine mirrors right-facing art back for leftward travel (#96).",
        "Mapping (animation <- this pack's actions.xml action):",
        ", ".join(chosen) + ".",
    ]
    return {
        "name": name,
        "personality": "",
        "license": None,
        "header": header,
        "animations": animations,
    }


FORMATS = ("petscodex", "shimeji")


def emit(pet, out, validate=True):
    """Write the Character Package: frames aligned per animation, manifest,
    personality — then prove it with `character::load`."""
    from PIL import Image

    animations = pet["animations"]
    unions = {}
    for animation, spec in animations.items():
        union = union_bbox(spec["frames"])
        if union is None:
            sys.exit(f"{animation}: every frame is fully transparent")
        unions[animation] = union

    # One resample factor for the whole character, from how tall it stands
    # idle, so every animation keeps the same proportions.
    idle = unions["idle"]
    factor, scale = import_scale(idle[3] - idle[1])
    mode = render_mode(*measure(animations["idle"]["frames"][0]))
    if factor != 1.0:
        resample = Image.LANCZOS if mode == "smooth" else Image.NEAREST
        for spec in animations.values():
            spec["frames"] = [
                frame.resize(
                    (round(frame.width * factor), round(frame.height * factor)),
                    resample,
                )
                for frame in spec["frames"]
            ]
        if mode == "pixelated":
            # The ADR-0006 quantisation pass: a resample drifts the palette,
            # so pin it back down. Untouched pixel art keeps its own.
            for spec in animations.values():
                spec["frames"] = [
                    frame.quantize(colors=64).convert("RGBA")
                    for frame in spec["frames"]
                ]
        unions = {a: union_bbox(spec["frames"]) for a, spec in animations.items()}

    # A uniform per-Character canvas, and per-frame placement on it: every
    # frame plants its lowest pixels on the canvas bottom (the shell anchors
    # art there — the #96 floating-sit lesson, applied to every frame the
    # way #96's hand pass did) and is registered horizontally to its
    # animation's first frame. Baked-in offsets are never kept, because the
    # Engine owns all motion — a fall descends by the contact point moving,
    # not by the art sinking in its cell — and generated sheets jitter
    # per-frame besides. The exception is a spec marked `registered`, whose
    # relative frame positions are deliberate: it shifts as a unit.
    metrics = {}
    for animation, spec in animations.items():
        boxes = [frame.getchannel("A").getbbox() for frame in spec["frames"]]
        if None in boxes:
            sys.exit(f"{animation}: frame {boxes.index(None)} is fully transparent")
        offsets = (
            [0] * len(boxes) if spec.get("registered") else x_offsets(spec["frames"])
        )
        left = min(box[0] + off for box, off in zip(boxes, offsets))
        right = max(box[2] + off for box, off in zip(boxes, offsets))
        union = unions[animation]
        height = (
            union[3] - union[1]
            if spec.get("registered")
            else max(box[3] - box[1] for box in boxes)
        )
        metrics[animation] = (boxes, offsets, right - left, left, height)

    canvas = (
        max(width for _, _, width, _, _ in metrics.values()),
        max(height for _, _, _, _, height in metrics.values()),
    )
    frames_dir = out / "frames"
    frames_dir.mkdir(parents=True)
    for animation, spec in animations.items():
        boxes, offsets, width, left, _ = metrics[animation]
        centering = (canvas[0] - width) // 2 - left
        anchored = animation_offset(unions[animation], canvas)
        for i, frame in enumerate(spec["frames"]):
            page = Image.new("RGBA", canvas, (0, 0, 0, 0))
            if spec.get("registered"):
                page.paste(frame, anchored)
            else:
                page.paste(frame, (centering + offsets[i], canvas[1] - boxes[i][3]))
            page.save(frames_dir / f"{animation}-{i}.png")

    header = pet["header"] + (
        [f"License: {pet['license']}."]
        if pet["license"]
        else [
            "License: none declared anywhere in the source ecosystem;",
            "accepted at import with --accept-license. A development asset",
            "unless a license surfaces — this repository's license claims",
            "do not cover the art.",
        ]
    )
    manifest = manifest_text(pet["name"], mode, scale, header, animations)
    (out / "character.manifest").write_text(manifest)
    if pet["personality"]:
        (out / "personality.txt").write_text(pet["personality"].strip() + "\n")

    print(f"{pet['name']}: {sum(len(s['frames']) for s in animations.values())} "
          f"frames on a {canvas[0]}x{canvas[1]} canvas, {mode} at scale {scale}")
    if validate:
        done = subprocess.run(
            ["cargo", "run", "-q", "-p", "ai-buddy-core", "--example", "validate",
             "--", str(out)],
            cwd=REPO,
        )
        if done.returncode != 0:
            sys.exit("character::load rejected the output; kept for inspection")


def require_pillow():
    """A missing Pillow is a setup problem, and gets setup instructions
    rather than a traceback."""
    try:
        import PIL  # noqa: F401 -- the import is itself the check
    except ModuleNotFoundError:
        sys.exit(
            "Pillow is not importable from this Python. Set up the venv and "
            "run the importer with it:\n"
            "  uv venv && uv pip install pillow\n"
            "  .venv/bin/python scripts/import-pet.py ..."
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("source", nargs="?", help="pet directory or zip")
    parser.add_argument("--format", choices=FORMATS, dest="ecosystem")
    parser.add_argument("-o", "--out", type=pathlib.Path)
    parser.add_argument("--accept-license", action="store_true",
                        help="proceed although the pet's license is unknown")
    parser.add_argument("--walk-row", type=int, choices=(1, 2), default=2,
                        help="petscodex: the sheet row to cut walk from. Row 2 "
                             "(petdex's running-left) is drawn heading right in "
                             "every pet sampled, so it is the default")
    parser.add_argument("--mirror-walk", action="store_true",
                        help="petscodex: flip the chosen walk row, for the pet "
                             "whose only clean walk heads left")
    parser.add_argument("--map", action="append", default=[], dest="mappings",
                        metavar="ANIM=ROW[:I,J,...]",
                        help="petscodex: cut an animation from another sheet "
                             "row (optionally naming its frames), for the pet "
                             "whose art strays from petdex's row semantics — "
                             "e.g. --map sleep=6:3 --map react=7")
    parser.add_argument("--force", action="store_true",
                        help="replace an existing output directory")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    require_pillow()
    if args.self_test:
        return self_test()
    if not (args.source and args.ecosystem and args.out):
        parser.error("source, --format and -o are required")

    source = pathlib.Path(args.source).expanduser()
    if zipfile.is_zipfile(source):
        unpacked = tempfile.mkdtemp(prefix="import-pet-")
        zipfile.ZipFile(source).extractall(unpacked)
        source = pathlib.Path(unpacked)
    if args.ecosystem == "petscodex":
        overrides = {}
        for text in args.mappings:
            animation, row, indices = parse_mapping(text)
            overrides[animation] = (row, indices)
        pet = read_petscodex(source, args.walk_row, args.mirror_walk, overrides)
    else:
        # A zip unpacks to a temp directory, so the pack's own name comes
        # from the path the user gave, not from where it landed.
        pet = read_shimeji(source, pathlib.Path(args.source).expanduser().stem)

    if pet["license"]:
        print(f"license: {pet['license']}")
    else:
        print("license: none declared — not in the pack, and the source "
              "ecosystem publishes no license metadata at all. Desktop-pet "
              "packs are typically fan art of third-party characters, so an "
              "import is a development asset: used locally, never shipped or "
              "redistributed.")
        if not args.accept_license:
            sys.exit("--accept-license records that judgment and proceeds")

    if args.out.exists():
        if not args.force:
            sys.exit(f"{args.out} exists; --force replaces it")
        shutil.rmtree(args.out)
    emit(pet, args.out)
    if args.ecosystem == "petscodex":
        print("review the output before shipping it: walk must head right "
              "(--walk-row picks the other row; --mirror-walk flips it), and "
              "every animation must read as its name — a pet whose art "
              "strays from the row semantics is recut with --map")
    else:
        print("review the output before shipping it — walk must head right")


def self_test():
    """The pure seams, then a synthetic sheet through the whole pipeline."""
    from PIL import Image

    assert detect_atlas(1536, 1872) == (1, 9, 192, 208)
    assert detect_atlas(3072, 3744) == (1, 9, 384, 416)
    assert detect_atlas(1536, 2288) == (2, 11, 192, 208)
    assert detect_atlas(1537, 1872) is None
    assert detect_atlas(100, 100) is None

    assert fps_for(6, 1100) == 5
    assert fps_for(8, 1060) == 8
    assert fps_for(4, 700) == 6
    assert fps_for(5, 840) == 6
    assert fps_for(8, 1220) == 7
    assert fps_for(1000, 1000) == MAX_FPS
    assert fps_for(1, 100000) == 1

    assert animation_offset((10, 20, 50, 90), (60, 100)) == (0, 10)
    assert animation_offset((0, 0, 60, 100), (60, 100)) == (0, 0)

    factor, scale = import_scale(174)
    assert scale == 1 and abs(factor - 120 / 174) < 1e-9
    assert import_scale(128) == (1.0, 1)
    assert import_scale(32) == (1.0, 4)
    assert import_scale(60) == (1.0, 2)

    assert render_mode(4097, 0.3) == "smooth"
    assert render_mode(16, 0.0) == "pixelated"

    assert parse_mapping("sleep=6:3") == ("sleep", 6, (3,))
    assert parse_mapping("react=7") == ("react", 7, None)

    # A synthetic v1 sheet, 8x8 cells: each declared frame is a 2x2 dot,
    # deliberately jittered per frame the way generated sheets are, so the
    # grounding and registration passes have something to undo.
    cell = 8
    sheet = Image.new("RGBA", (ATLAS_COLUMNS * cell, 9 * cell), (0, 0, 0, 0))
    for state, (row, count, _) in PETSCODEX_ROWS.items():
        for c in range(count):
            x = c * cell + 3 + (c % 3)
            y = row * cell + 2 + (row % 3) + (c % 2)
            for dx in range(2):
                for dy in range(2):
                    sheet.putpixel((x + dx, y + dy), (200, 10 + row, 10, 255))
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        source = tmp / "pet"
        source.mkdir()
        (source / "pet.json").write_text(json.dumps({
            "id": "dot", "displayName": "Dot", "description": "a test dot",
            "spritesheetPath": "sheet.png",
        }))
        # detect_atlas refuses square cells; stretch to the canonical grid.
        tall = sheet.resize((ATLAS_COLUMNS * 192, 9 * 208), Image.NEAREST)
        tall.save(source / "sheet.png")

        pet = read_petscodex(source)
        assert set(pet["animations"]) == {
            "idle", "waiting", "walk", "talk", "fall", "land", "hold",
            "react", "sit", "sleep",
        }
        assert pet["animations"]["land"]["loop"] == "once"
        assert pet["animations"]["waiting"]["variant_of"] == "idle"
        assert pet["license"] is None

        # --walk-row swaps the walk source row (the synthetic sheet colors
        # each row differently, so the swap shows in the pixels), and
        # --mirror-walk flips the chosen row.
        other = read_petscodex(source, walk_row=1)
        walk = pet["animations"]["walk"]["frames"][0]
        assert walk.tobytes() != other["animations"]["walk"]["frames"][0].tobytes()
        mirrored = read_petscodex(source, mirror_walk=True)
        from PIL import ImageOps
        flipped_back = ImageOps.mirror(mirrored["animations"]["walk"]["frames"][0])
        assert walk.tobytes() == flipped_back.tobytes()

        out = tmp / "character"
        emit(pet, out, validate=False)
        manifest = (out / "character.manifest").read_text()
        for required in ("idle", "walk", "fall", "land", "sit", "sleep",
                         "react", "talk", "hold"):
            assert f"[animations.{required}]" in manifest
        assert (out / "personality.txt").read_text().strip() == "a test dot"

        # Grounding and registration: every frame of every animation lands
        # its dot on the canvas bottom at one x — the jitter baked into the
        # sheet is gone, so nothing drifts inside a cycle.
        for animation in set(pet["animations"]) - {"sleep"}:
            placed = {
                Image.open(path).getchannel("A").getbbox()
                for path in sorted(out.glob(f"frames/{animation}-*.png"))
            }
            assert len(placed) == 1, f"{animation} drifts: {placed}"
            frame = Image.open(out / "frames" / f"{animation}-0.png")
            assert frame.getchannel("A").getbbox()[3] == frame.height, animation
        # The sleep breath survives the grounding pass: frame 1 keeps its
        # deliberate one-pixel lift over a planted frame 0.
        sleep0 = Image.open(out / "frames" / "sleep-0.png").getchannel("A").getbbox()
        sleep1 = Image.open(out / "frames" / "sleep-1.png").getchannel("A").getbbox()
        assert sleep0[3] - sleep1[3] == 1

        # --map cuts an animation from the named row: row 5's dots carry
        # row 5's color.
        remapped = read_petscodex(source, overrides={"hold": (5, (0, 1))})
        hold = remapped["animations"]["hold"]["frames"][0]
        assert (200, 15, 10, 255) in {color for _, color in hold.getcolors()}
        assert len(remapped["animations"]["hold"]["frames"]) == 2

    # A synthetic shimeji pack: three 8x8 frames with one marker pixel near
    # the left edge, and an actions.xml with no lie-down and no wave, so the
    # sleep synthesis and the talk<-Stand fallback both run.
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        (tmp / "img").mkdir()
        (tmp / "conf").mkdir()
        for n in (1, 2, 3):
            shime = Image.new("RGBA", (8, 8), (0, 0, 0, 0))
            shime.putpixel((1, 4 + n % 2), (10 * n, 0, 0, 255))
            shime.save(tmp / "img" / f"shime{n}.png")
        (tmp / "conf" / "actions.xml").write_text("""<?xml version="1.0"?>
<Mascot xmlns="http://www.group-finity.com/Mascot"><ActionList>
<Action Name="Stand" Type="Stay"><Animation><Pose Image="/shime1.png" Duration="250"/></Animation></Action>
<Action Name="Walk" Type="Move"><Animation>
  <Pose Image="/shime1.png" Duration="6"/><Pose Image="/shime2.png" Duration="6"/>
  <Pose Image="/shime1.png" Duration="6"/><Pose Image="/shime3.png" Duration="7"/>
</Animation></Action>
<Action Name="Falling" Type="Embedded"><Animation><Pose Image="/shime2.png" Duration="250"/></Animation></Action>
<Action Name="Bouncing" Type="Animate"><Animation><Pose Image="/shime3.png" Duration="4"/><Pose Image="/shime1.png" Duration="4"/></Animation></Action>
<Action Name="Sit" Type="Stay"><Animation><Pose Image="/shime2.png" Duration="250"/></Animation></Action>
<Action Name="Pinched" Type="Embedded"><Animation><Pose Image="/shime3.png" Duration="5"/></Animation></Action>
<Action Name="Tripping" Type="Animate"><Animation><Pose Image="/shime2.png" Duration="8"/><Pose Image="/shime3.png" Duration="8"/></Animation></Action>
<Action Name="Fall" Type="Sequence"/>
</ActionList></Mascot>""")

        pack = read_shimeji(tmp, "Tester")
        assert set(pack["animations"]) == {
            "idle", "walk", "fall", "land", "sit", "sleep", "react", "talk",
            "hold",
        }, set(pack["animations"])
        # Mirrored: the marker pixel at x=1 lands at x=6 on an 8-wide frame.
        idle = pack["animations"]["idle"]["frames"][0]
        assert idle.getpixel((6, 5))[3] == 255 and idle.getpixel((1, 5))[3] == 0
        # Walk repeats shime1 (poses 1,2,1,3 over two written frames of it);
        # 25 ticks of 40ms are one second, so four poses make fps 4.
        walk = pack["animations"]["walk"]
        assert len(walk["frames"]) == 3 and walk["order"] == [0, 1, 0, 2]
        assert walk["fps"] == 4
        assert pack["animations"]["land"]["loop"] == "once"
        assert pack["animations"]["react"]["loop"] == "once"
        # No lie-down action: sleep is the synthesized idle-breath pair.
        assert len(pack["animations"]["sleep"]["frames"]) == 2

        out = tmp / "character"
        emit(pack, out, validate=False)
        manifest = (out / "character.manifest").read_text()
        assert manifest.count('"frames/walk-') == 4  # the repeated path
        assert (out / "frames" / "walk-2.png").exists()
        assert not (out / "frames" / "walk-3.png").exists()

    # A pack of bare shime PNGs and no actions.xml — shimejishop distributes
    # these — rides Shimeji-ee's standard conf instead.
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        named = {
            image
            for poses in SHIMEJI_DEFAULT_ACTIONS.values()
            for image, _ in poses
        }
        for image in named:
            shime = Image.new("RGBA", (8, 8), (0, 0, 0, 0))
            shime.putpixel((2, 6), (99, 0, 0, 255))
            shime.save(tmp / image)
        pack = read_shimeji(tmp, "Bare")
        assert "climb" in pack["animations"]
        assert pack["animations"]["sleep"]["variant_of"] is None
        assert len(pack["animations"]["hold"]["frames"]) == len(
            {image for image, _ in SHIMEJI_DEFAULT_ACTIONS["Pinched"]}
        )

    print("self-test: ok")


if __name__ == "__main__":
    main()
