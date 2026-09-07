#!/usr/bin/env python3
"""Build the overlay-expression page from src/bubble.js and src/main.css.

    python3 scripts/make-expression-page.py --out _site
    python3 scripts/make-expression-page.py --self-check

A Generated page under ADR-0011: the published HTML must load the overlay's
bubble module and the bubble rules sliced from main.css, so a reading-time
formula restated on the page cannot deploy. Malformed or missing bubble.js
fails the build the way a bad Character Manifest fails the gallery.

Pure standard library. The CSS slice is text, not a parser, so it cannot
quietly pick up the overlay's transparent fullscreen rules sitting above
`.bubble`.
"""

import argparse
import pathlib
import re
import shutil
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
BUBBLE_JS = ROOT / "src" / "bubble.js"
MAIN_CSS = ROOT / "src" / "main.css"
SHELL = ROOT / "docs" / "design" / "expression.html"

EXPORTS = (
    "export function bubbleDuration",
    "export function createBubbleMachine",
    "export const THINKING_GRACE_MS",
    "export const THINKING_MIN_HOLD_MS",
)

FORMULA = re.compile(r"900\s*\+\s*55")
RESTATED_TIMERS = re.compile(
    r"THINKING_GRACE_MS\s*=\s*250|THINKING_MIN_HOLD_MS\s*=\s*600"
)
NOINDEX = re.compile(r'<meta\s+name="robots"\s+content="noindex"', re.I)


class Malformed(Exception):
    """Input the page cannot describe truthfully."""


def require_module(source):
    """Refuse a bubble.js that is not the overlay's machine."""
    if not source.strip():
        raise Malformed("src/bubble.js is empty")
    for token in EXPORTS:
        name = token.rsplit(" ", 1)[-1]
        if token not in source:
            raise Malformed(f"src/bubble.js does not export {name}")


def extract_bubble_css(css):
    """The bubble rules only.

    Copying main.css wholesale paints a transparent fullscreen overlay onto a
    document that has to stay a readable page. The slice starts at `.bubble {`
    and ends when the thinking-bounce keyframes close — those two markers are
    the overlay's speech and thinking surfaces, and nothing the overlay uses
    for layout sits between them today.
    """
    start = css.find(".bubble {")
    if start < 0:
        raise Malformed("src/main.css declares no .bubble rule")
    key = css.find("@keyframes thinking-bounce", start)
    if key < 0:
        raise Malformed("src/main.css declares no thinking-bounce keyframes")
    brace = css.find("{", key)
    if brace < 0:
        raise Malformed("src/main.css thinking-bounce keyframes never open")
    depth = 0
    for index, char in enumerate(css[brace:], brace):
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                sliced = css[start : index + 1].strip() + "\n"
                if "html," in sliced or "background: transparent" in sliced:
                    raise Malformed("bubble CSS slice picked up overlay layout")
                return sliced
    raise Malformed("src/main.css thinking-bounce keyframes never close")


def require_honest_page(html):
    """The shell must operate the module, not describe it."""
    if 'type="module"' not in html:
        raise Malformed("expression.html is not a module page")
    if 'from "./bubble.js"' not in html and "from './bubble.js'" not in html:
        raise Malformed("expression.html does not load src/bubble.js")
    if "bubbleDuration(" not in html:
        raise Malformed("expression.html never calls bubbleDuration")
    if "createBubbleMachine(" not in html:
        raise Malformed("expression.html never calls createBubbleMachine")
    if FORMULA.search(html):
        raise Malformed("expression.html restates reading-time arithmetic")
    if RESTATED_TIMERS.search(html):
        raise Malformed("expression.html restates grace or hold timers")
    if NOINDEX.search(html):
        raise Malformed("expression.html is Generated and must stay indexed")
    if "Generated from src/bubble.js at deploy" not in html:
        raise Malformed("expression.html does not name its Generated source")


def assemble(src_root, out):
    bubble_path = src_root / "src" / "bubble.js"
    css_path = src_root / "src" / "main.css"
    shell_path = src_root / "docs" / "design" / "expression.html"

    if not bubble_path.is_file():
        raise Malformed("src/bubble.js is missing")
    source = bubble_path.read_text(encoding="utf-8")
    require_module(source)

    if not css_path.is_file():
        raise Malformed("src/main.css is missing")
    css = extract_bubble_css(css_path.read_text(encoding="utf-8"))

    if not shell_path.is_file():
        raise Malformed("docs/design/expression.html is missing")
    html = shell_path.read_text(encoding="utf-8")
    require_honest_page(html)

    out.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(bubble_path, out / "bubble.js")
    (out / "bubble.css").write_text(css, encoding="utf-8")
    (out / "expression.html").write_text(html, encoding="utf-8")
    return out / "expression.html"


def _tree(scratch, bubble=None, css=None, html=None):
    root = scratch / "src_root"
    (root / "src").mkdir(parents=True)
    (root / "docs" / "design").mkdir(parents=True)
    if bubble is not None:
        (root / "src" / "bubble.js").write_text(bubble, encoding="utf-8")
    if css is not None:
        (root / "src" / "main.css").write_text(css, encoding="utf-8")
    if html is not None:
        (root / "docs" / "design" / "expression.html").write_text(html, encoding="utf-8")
    return root


def self_check():
    """Prove malformed bubble.js cannot ship, and a good tree stays Generated."""
    good_js = BUBBLE_JS.read_text(encoding="utf-8")
    good_css = MAIN_CSS.read_text(encoding="utf-8")
    good_html = SHELL.read_text(encoding="utf-8")

    def raises(root, out):
        try:
            assemble(root, out)
        except Malformed as caught:
            return str(caught)
        return None

    with tempfile.TemporaryDirectory() as scratch:
        scratch = pathlib.Path(scratch)
        out = scratch / "out"

        missing = _tree(scratch / "missing", css=good_css, html=good_html)
        assert raises(missing, out / "missing"), "missing bubble.js built a page anyway"

        empty = _tree(scratch / "empty", bubble="", css=good_css, html=good_html)
        assert raises(empty, out / "empty"), "empty bubble.js built a page anyway"

        mute = _tree(
            scratch / "mute",
            bubble="export function wrapText() {}\n",
            css=good_css,
            html=good_html,
        )
        assert raises(mute, out / "mute"), "bubble.js without the machine built a page anyway"

        bare = _tree(scratch / "bare", bubble=good_js, css="body { color: red; }\n", html=good_html)
        assert raises(bare, out / "bare"), "main.css without .bubble built a page anyway"

        described = good_html.replace("bubbleDuration(", "/* duration = 900 + 55 * length */ bubbleDuration(")
        fake = _tree(scratch / "described", bubble=good_js, css=good_css, html=described)
        assert raises(fake, out / "described"), "a page restating the formula built anyway"

        page = assemble(ROOT, out / "good")
        html = page.read_text(encoding="utf-8")
        require_honest_page(html)
        css = (out / "good" / "bubble.css").read_text(encoding="utf-8")
        assert ".bubble.visible" in css
        assert ".thinking-dots" in css
        copied = (out / "good" / "bubble.js").read_text(encoding="utf-8")
        assert copied == good_js

    print("self-check: bubble.js exports, CSS slice, honest page, 6 checks passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=pathlib.Path, default=ROOT / "_site",
                        help="directory to write expression.html, bubble.js and bubble.css into")
    parser.add_argument("--self-check", action="store_true",
                        help="run the generator's own checks and exit")
    arguments = parser.parse_args()

    if arguments.self_check:
        try:
            self_check()
        except FileNotFoundError as missing:
            sys.exit(f"overlay expression: {missing}")
        except AssertionError as failed:
            sys.exit(f"overlay expression: self-check failed: {failed}")
        return

    try:
        page = assemble(ROOT, arguments.out)
    except Malformed as broken:
        sys.exit(f"overlay expression: {broken}")
    print(f"{page}: bubble.js and bubble.css")


if __name__ == "__main__":
    main()
