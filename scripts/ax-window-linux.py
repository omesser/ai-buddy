#!/usr/bin/env python3
"""Dump one Fidget window's AT-SPI tree as `role|name` lines.

The scenario greps `button|<glyph> thinking`. pyatspi role names are folded
onto that short role so the assert does not depend on "push button" vs "button".

Usage: ax-window-linux.py dump PID TITLE
Exit 2 when pyatspi is missing. Exit 1 when that window is not in the tree.
"""

import sys

ROLE = {
    "push button": "button",
    "toggle button": "button",
    "frame": "frame",
    "label": "label",
    "link": "link",
}


def dump(acc, depth=0):
    if depth > 30:
        return
    try:
        raw = acc.getRoleName() or "unknown"
    except Exception:
        raw = "unknown"
    role = ROLE.get(raw, raw.replace(" ", "-"))
    try:
        name = (acc.name or "").replace("\n", "\\n")
    except Exception:
        name = ""
    print(f"{role}|{name}")
    try:
        count = acc.childCount
    except Exception:
        return
    for i in range(count):
        try:
            dump(acc.getChildAtIndex(i), depth + 1)
        except Exception:
            pass


def main(argv):
    if len(argv) != 4 or argv[1] != "dump":
        print("usage: ax-window-linux.py dump PID TITLE", file=sys.stderr)
        return 2
    pid = int(argv[2])
    title = argv[3].lower()
    try:
        import pyatspi
    except ImportError:
        print("pyatspi is not installed", file=sys.stderr)
        return 2
    desktop = pyatspi.Registry.getDesktop(0)
    window = None
    for i in range(desktop.childCount):
        app = desktop.getChildAtIndex(i)
        try:
            if app.get_process_id() != pid:
                continue
        except Exception:
            continue
        for j in range(app.childCount):
            win = app.getChildAtIndex(j)
            try:
                name = win.name or ""
            except Exception:
                name = ""
            if title in name.lower():
                window = win
                break
        if window is not None:
            break
    if window is None:
        print(f"no window titled {argv[3]} for pid {pid}", file=sys.stderr)
        return 1
    dump(window)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
