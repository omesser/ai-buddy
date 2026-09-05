#!/usr/bin/env python3
"""Build the Character gallery page from characters/.

    python3 scripts/make-character-gallery.py --out _site
    python3 scripts/make-character-gallery.py --self-check

Reads every Character Manifest, copies the frames the manifests name into
`<out>/characters/`, and writes `<out>/characters.html` — the page shell in
docs/design/characters.html with its data block substituted. A Generated page
under ADR-0011: the manifests are the source of the frame count, fps, loop,
render_mode and scale the page shows, so a page that disagrees with a package
cannot be deployed.

Pure standard library. tomllib reads the manifests and the frames are copied
byte for byte, so nothing here needs Pillow or a TOML dependency installed.

This script decides what gets published. .github/workflows/pages.yml names
files one by one for the hand-written pages, and that reviewed list is the
security model; it cannot name a few hundred PNGs. So the workflow names this
generator instead, and the guarantee moves here: only the files a manifest
names as frames are copied, only if they are PNGs inside their own package,
and the workflow re-checks every extension in _site afterwards.

Malformed input fails the build rather than reaching the page. A required
Animation a manifest simply does not declare is different: that is a fact
about the package the gallery exists to show, so it renders as a missing tile.
The loader already refuses to install such a package (crates/core/src/
character.rs), so nothing here is the only thing standing between it and a
user.
"""

import argparse
import json
import pathlib
import re
import shutil
import struct
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
CHARACTERS = ROOT / "characters"
RUST = ROOT / "crates" / "core" / "src" / "character.rs"
SHELL = ROOT / "docs" / "design" / "characters.html"
PLACEHOLDER = '{"characters": [], "required": [], "defaults": {}}'

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


class Malformed(Exception):
    """A package the gallery cannot describe truthfully."""


# --------------------------------------------------------------------------
# What the Engine says, read from the Engine
# --------------------------------------------------------------------------


def from_rust(source):
    """The Required Animation Set and the manifest defaults, from character.rs.

    Retyping the nine names here would make the page's idea of completeness a
    second declaration that nothing keeps in step with the loader's. Same for
    the two defaults: a package that declares no fps plays at eight because
    that constant says so, and a gallery hardcoding eight would keep claiming
    it after someone changed it.
    """
    block = re.search(r"REQUIRED_ANIMATIONS:\s*\[&str;\s*(\d+)\]\s*=\s*\[(.*?)\];", source, re.S)
    if not block:
        raise Malformed(f"{RUST.name} declares no REQUIRED_ANIMATIONS array")
    required = re.findall(r'"([^"]+)"', block.group(2))
    if len(required) != int(block.group(1)):
        raise Malformed(f"REQUIRED_ANIMATIONS says {block.group(1)} names and lists {len(required)}")

    def constant(name):
        found = re.search(rf"pub const {name}:\s*u32\s*=\s*(\d+);", source)
        if not found:
            raise Malformed(f"{RUST.name} declares no {name}")
        return int(found.group(1))

    return required, {"fps": constant("DEFAULT_FPS"), "scale": constant("DEFAULT_SCALE")}


# --------------------------------------------------------------------------
# One package
# --------------------------------------------------------------------------


def provenance(text):
    """The manifest's leading comment block, verbatim.

    Most packages cite a third-party art source in that comment, and three
    say this repository's license does not cover the art. The gallery is a
    public, indexed URL, so it repeats what the manifest says instead of a
    summary of it — a paraphrase is how a license caveat quietly becomes a
    weaker one. A package whose manifest opens with no comment gets no block,
    rather than a line asserting the art is ours.
    """
    lines = []
    for line in text.splitlines():
        if not line.startswith("#"):
            break
        lines.append(line[1:].removeprefix(" "))
    return "\n".join(lines).strip()


def frame(package, declared, art_root):
    """Copy one frame into the site tree and measure it.

    The path check is the reason this function exists rather than a shutil
    one-liner: a manifest is data, and this script is what stands between it
    and a public URL now that the workflow no longer names each file.
    """
    parts = pathlib.PurePosixPath(declared).parts
    if declared.startswith("/") or ".." in parts:
        raise Malformed(f"{package.name}: frame {declared!r} points outside the package")
    if not declared.endswith(".png"):
        raise Malformed(f"{package.name}: frame {declared!r} is not a .png")

    source = package / pathlib.PurePosixPath(declared)
    if not source.is_file():
        raise Malformed(f"{package.name}: declares frame {declared!r}, which the package lacks")
    art = source.read_bytes()
    if art[:8] != PNG_SIGNATURE or art[12:16] != b"IHDR":
        raise Malformed(f"{package.name}: frame {declared!r} is not a PNG")
    width, height = struct.unpack(">II", art[16:24])

    destination = art_root / package.name / pathlib.PurePosixPath(declared)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(art)
    return {"src": f"characters/{package.name}/{declared}", "w": width, "h": height}


def character(package, required, defaults, art_root):
    manifest = package / "character.manifest"
    try:
        text = manifest.read_text(encoding="utf-8")
        declared = tomllib.loads(text)
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as broken:
        raise Malformed(f"{package.name}: {manifest.name} does not read as TOML — {broken}") from broken

    animations = declared.get("animations", {})
    if not isinstance(animations, dict):
        raise Malformed(f"{package.name}: [animations] is not a table")

    strip = []
    for name in required:
        animation = animations.get(name)
        if animation is None:
            strip.append({"name": name, "missing": True})
            continue
        frames = animation.get("frames")
        if not isinstance(frames, list) or not frames:
            raise Malformed(f"{package.name}: {name!r} declares no frames")
        strip.append({
            "name": name,
            "fps": animation.get("fps", defaults["fps"]),
            # The Engine holds the last frame of a `once` Animation forever;
            # the page does the same and offers a replay.
            "loop": animation.get("loop") != "once",
            "frames": [frame(package, path, art_root) for path in frames],
        })

    return {
        "dir": package.name,
        "name": declared.get("name") or package.name,
        "smooth": declared.get("render_mode") == "smooth",
        "scale": declared.get("scale", defaults["scale"]),
        "provenance": provenance(text),
        "animations": strip,
    }


# --------------------------------------------------------------------------
# The page
# --------------------------------------------------------------------------


def gallery(characters_root, rust_source, out):
    required, defaults = from_rust(rust_source)
    packages = sorted(p for p in characters_root.iterdir() if (p / "character.manifest").is_file())
    if not packages:
        raise Malformed(f"{characters_root} holds no Character Package")

    art_root = out / "characters"
    data = {
        "characters": [character(p, required, defaults, art_root) for p in packages],
        "required": required,
        "defaults": defaults,
    }

    # Escaped so the JSON can never close the <script> element it sits in, and
    # so a manifest comment in any language survives the trip.
    block = json.dumps(data, ensure_ascii=True).replace("<", "\\u003c")
    shell = SHELL.read_text(encoding="utf-8")
    if shell.count(PLACEHOLDER) != 1:
        raise Malformed(f"{SHELL.name} holds no single data placeholder to substitute")
    page = out / "characters.html"
    page.write_text(shell.replace(PLACEHOLDER, block), encoding="utf-8")
    return data, page


# --------------------------------------------------------------------------
# The check
# --------------------------------------------------------------------------


def self_check():
    """Prove the three behaviors the page's honesty rests on.

    A generator that silently emits a broken page is the failure mode worth
    designing out, and the two halves of that are inseparable: malformed input
    has to stop the build, and a package that is merely incomplete has to
    reach the page as incomplete. Asserting both here costs less than a test
    harness this repository does not otherwise have for Python.
    """
    art = (CHARACTERS / "nim" / "frames" / "idle-0.png").read_bytes()
    required, defaults = from_rust(RUST.read_text(encoding="utf-8"))
    body = "\n".join(
        f'[animations.{name}]\nframes = ["frames/{name}-0.png"]' for name in required
    )

    def package(root, manifest, frames=("idle-0.png",)):
        root.mkdir(parents=True, exist_ok=True)
        (root / "character.manifest").write_text(manifest, encoding="utf-8")
        (root / "frames").mkdir(exist_ok=True)
        for name in frames:
            (root / "frames" / name).write_bytes(art)

    def raises(root, out):
        try:
            gallery(root, RUST.read_text(encoding="utf-8"), out)
        except Malformed as caught:
            return str(caught)
        return None

    with tempfile.TemporaryDirectory() as scratch:
        scratch = pathlib.Path(scratch)
        out = scratch / "out"
        out.mkdir()

        junk = scratch / "junk"
        package(junk / "broken", "name = = =\n")
        assert raises(junk, out), "a manifest that is not TOML built a page anyway"

        absent = scratch / "absent"
        package(absent / "gappy", f'name = "Gappy"\n{body}\n')
        assert raises(absent, out), "a declared frame the package lacks built a page anyway"

        escape = scratch / "escape"
        package(
            escape / "sneaky",
            'name = "Sneaky"\n[animations.idle]\nframes = ["../../../etc/hosts.png"]\n',
        )
        assert raises(escape, out), "a frame path leaving the package built a page anyway"

        short = scratch / "short"
        names = [n for n in required if n != "walk"]
        package(
            short / "shortish",
            'name = "Shortish"\n'
            + "\n".join(f'[animations.{n}]\nframes = ["frames/{n}-0.png"]' for n in names),
            frames=[f"{n}-0.png" for n in names],
        )
        data, _ = gallery(short, RUST.read_text(encoding="utf-8"), out)
        walk = next(a for a in data["characters"][0]["animations"] if a["name"] == "walk")
        assert walk.get("missing"), "an undeclared Animation did not reach the page as missing"
        assert len(data["characters"][0]["animations"]) == len(required), "the strip lost a slot"

    print(f"self-check: {len(required)} Required Animations, "
          f"defaults fps={defaults['fps']} scale={defaults['scale']}, 4 checks passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=pathlib.Path, default=ROOT / "_site",
                        help="directory to write characters.html and characters/ into")
    parser.add_argument("--self-check", action="store_true",
                        help="run the generator's own checks and exit")
    arguments = parser.parse_args()

    if arguments.self_check:
        self_check()
        return

    out = arguments.out
    out.mkdir(parents=True, exist_ok=True)
    shutil.rmtree(out / "characters", ignore_errors=True)
    try:
        data, page = gallery(CHARACTERS, RUST.read_text(encoding="utf-8"), out)
    except Malformed as broken:
        sys.exit(f"character gallery: {broken}")

    frames = sum(len(a["frames"]) for c in data["characters"] for a in c["animations"]
                 if not a.get("missing"))
    missing = sum(1 for c in data["characters"] for a in c["animations"] if a.get("missing"))
    print(f"{page}: {len(data['characters'])} Characters, {frames} frames"
          + (f", {missing} Required Animations missing" if missing else ""))


if __name__ == "__main__":
    main()
