#!/usr/bin/env bash
#
# Machine-checkable verification for the overlay window and the frame loop.
#
# Deliberately not a cargo test: every check here needs a real desktop, a real
# window server and a running app, so it is slow, it is macOS-only, and it
# cannot run in CI. Run it by hand when the overlay, the platform layer or the
# frame loop changes. `cargo test` stays fast and pure.
#
# What this cannot check: whether a click actually passes through to the window
# underneath, and whether typing elsewhere survives a click on the sprite. Those
# need a human. See the checklist in README.md.
#
# Usage: scripts/verify-overlay.sh [--keep]
#   --keep   leave the app running afterwards, with tracing on

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

# An orphaned overlay is always-on-top and has no window controls, so an
# interrupted run would otherwise leave something on screen that is awkward to
# get rid of. --keep opts out for the app, never for the prop window: that one
# is scaffolding and nobody wants it left behind.
trap 'pkill -f perch-window.swift 2> /dev/null;
      [ "$KEEP" = "1" ] || pkill -f "target/debug/ai-buddy" 2> /dev/null' EXIT INT TERM

STAMP=$(date +%Y%m%d-%H%M%S)
OUT=".verify/$STAMP"
mkdir -p "$OUT"

now_ms() { python3 -c 'import time;print(int(time.time()*1000))'; }

# Waits for a log to say something, rather than sleeping a guessed interval:
# every timing here belongs to the app or the window server, not to us.
await() { # $1=file  $2=grep -E pattern  $3=attempts, a quarter-second each
  for _ in $(seq 1 "$3"); do
    grep -qE "$2" "$1" 2> /dev/null && return 0
    perl -e 'select(undef,undef,undef,0.25)'
  done
  return 1
}

echo "Building..."
if ! cargo build 2>&1 | tail -3; then
  echo "FAIL: build"
  exit 1
fi

# ---------------------------------------------------------------------------
# A Perch to aim the sprite at.
#
# The sprite starts in the middle of the usable part of the first display and
# falls, so a window whose top edge is below that point is something it can
# land on. It has to
# exist before the app does: the fall takes under a second, and a window that
# arrives afterwards is above the sprite, which is not a surface from below.
# ---------------------------------------------------------------------------
swift scripts/inspect-window.swift > "$OUT/desktop.json" 2> "$OUT/desktop.err" || {
  echo "FAIL: inspector"
  cat "$OUT/desktop.err"
  exit 1
}

RECTS=$(
  python3 - "$OUT/desktop.json" << 'PY'
import json, sys

# Where the app puts the sprite, computed the way the app computes it:
# `snapshot::starting_position` takes the middle of the *usable* frame of the
# first display. Usable, not the whole frame, so the menu bar's height and the
# Dock's edge and size shift this start point, and props measured from the
# frame's centre instead would sit at a different distance from the sprite on
# every desktop. First display, because tao's `available_monitors` and this
# script's inspector both enumerate `CGGetActiveDisplayList`, which reports the
# main display first.
u = json.load(open(sys.argv[1]))["displays"][0]["usable"]
sprite_x = u["x"] + u["w"] / 2
sprite_y = u["y"] + u["h"] / 2

# Every offset below is a fraction of the room between the sprite and the floor
# rather than a fixed number of points. A fixed drop is calibrated to one
# display's height and one Dock: on a shorter screen it puts a prop's top edge
# past the bottom, where macOS refuses to place a titled window, so the prop
# never steps and a working frame loop reads as broken.
#
# What is left assumed: that the room below the sprite is deep enough for the
# Perch to step down 80 points and stay on screen, which needs a usable height
# of roughly 350 points. Every display macOS runs on clears that.
room = u["h"] / 2
width = min(400.0, u["w"])
left = int(sprite_x - width / 2)

# The Perch: halfway from the sprite to the floor, and wide enough around the
# sprite that it is over the window and has somewhere to fall from.
print(left, int(sprite_y + room / 2), int(width), 240)
# The furniture: between the sprite's start and that Perch, so the sprite falls
# through it on the way down and the trace says whether it stopped.
print(left, int(sprite_y + room / 6), int(width), 120)
PY
)
PERCH_RECT=$(echo "$RECTS" | sed -n 1p)
OVER_RECT=$(echo "$RECTS" | sed -n 2p)

# ---------------------------------------------------------------------------
# A window the sprite must NOT land on.
#
# The Dock and the menu bar are not Perches, and the sprite has to fall past
# them. Neither can be used to check that: the real furniture all has its top
# edge at the top of the screen, where a falling sprite never meets it. A prop
# opened at the Dock's own window level, in the sprite's way, does meet it.
# ---------------------------------------------------------------------------
DOCK_LEVEL=20
echo "Opening a prop at window level $DOCK_LEVEL at $OVER_RECT to fall through..."
# shellcheck disable=SC2086  # four separate arguments, deliberately
swift scripts/perch-window.swift $OVER_RECT $DOCK_LEVEL > "$OUT/over.log" 2>&1 &
OVER_PID=$!
await "$OUT/over.log" '^\{' 40 || {
  echo "FAIL: elevated prop window never reported its bounds"
  cat "$OUT/over.log"
  exit 1
}

echo "Opening a prop window at $PERCH_RECT to perch on..."
# shellcheck disable=SC2086  # four separate arguments, deliberately
swift scripts/perch-window.swift $PERCH_RECT > "$OUT/perch.log" 2>&1 &
PERCH_PID=$!
await "$OUT/perch.log" '^\{' 40 || {
  echo "FAIL: prop window never reported its bounds"
  cat "$OUT/perch.log"
  exit 1
}

pkill -f 'target/debug/ai-buddy' 2> /dev/null
AI_BUDDY_TRACE_HITTEST=1 AI_BUDDY_TRACE_FRAMES=1 \
  ./target/debug/ai-buddy > "$OUT/app.log" 2>&1 &
APP_PID=$!

# Wait for the startup line rather than sleeping a guessed interval.
for _ in $(seq 1 60); do
  grep -q '^overlay:' "$OUT/app.log" 2> /dev/null && break
  kill -0 "$APP_PID" 2> /dev/null || {
    echo "FAIL: app exited"
    cat "$OUT/app.log"
    exit 1
  }
  perl -e 'select(undef,undef,undef,0.25)'
done

# Land on the prop, then wait for a step it takes *after* landing, so the check
# never races the app's startup.
await "$OUT/app.log" '^frame: [0-9]+ Perched' 60 ||
  echo "  (the sprite never perched - the checks below will say so)"

# It has done its job by now: the sprite has fallen past it, and it is drawn
# above the overlay, so leaving it up would put it over the screenshot below.
kill "$OVER_PID" 2> /dev/null

swift scripts/inspect-window.swift > "$OUT/window.json" 2> "$OUT/window.err" ||
  {
    echo "FAIL: inspector"
    cat "$OUT/window.err"
  }

PERCH_LINES=$(grep -c '^{' "$OUT/perch.log")
for _ in $(seq 1 40); do
  [ "$(grep -c '^{' "$OUT/perch.log")" -gt "$PERCH_LINES" ] && break
  perl -e 'select(undef,undef,undef,0.25)'
done
perl -e 'select(undef,undef,undef,1.5)' # long enough to fall the step and settle

echo "Closing the prop window..."
CLOSED_MS=$(now_ms)
kill "$PERCH_PID" 2> /dev/null
perl -e 'select(undef,undef,undef,2.5)' # long enough to notice, fall and settle

echo ""
echo "Frame loop:"
python3 - "$OUT" "$CLOSED_MS" << 'PY'
import json, re, sys

out, closed_ms = sys.argv[1], int(sys.argv[2])

# docs/SPEC.md: WindowSource is read at approximately 10Hz. The slack covers one
# engine tick plus however long pkill and this script's own timestamp take.
POLL_MS, SLACK_MS = 100, 150

frames = [
    (int(m[1]), m[2], float(m[3]), float(m[4]))
    for m in (
        re.match(r"frame: (\d+) (\w+) pos\((-?\d+),(-?\d+)\)", line)
        for line in open(f"{out}/app.log")
    )
    if m
]
steps = [json.loads(line) for line in open(f"{out}/perch.log") if line.startswith("{")]
displays = json.load(open(f"{out}/desktop.json"))["displays"]

fails = []

def check(ok, label, detail=""):
    print(f"  {'PASS' if ok else 'FAIL'}  {label}{'  ' + detail if detail else ''}")
    if not ok:
        fails.append(label)

def first(predicate, of=frames):
    return next((f for f in of if predicate(f)), None)

if not frames or not steps:
    print("  FAIL  the app traced no frames" if not frames
          else "  FAIL  the prop window reported nothing")
    sys.exit(1)

descent = [f for f in frames if f[1] == "Falling"][:10]
check(len(descent) >= 5 and descent[-1][3] > descent[0][3] + 10,
      "the sprite falls under gravity",
      f"{descent[0][3]:.0f} -> {descent[-1][3]:.0f} over {len(descent)} frames")
check(all(b[3] > a[3] for a, b in zip(descent, descent[1:])),
      "every frame of a fall is lower than the last")

# The prop's own reported top edge, from the window server rather than from what
# it was asked for: a titled window's frame is taller than its content rect.
landed = first(lambda f: f[1] == "Perched" and abs(f[3] - steps[0]["y"]) <= 1)
check(landed is not None,
      "it comes to rest on a real window's top edge",
      f"window top y={steps[0]['y']:.0f}")

# The first step the prop took once the sprite was already perched on it.
step = first(lambda s: landed and s["at_ms"] > landed[0], steps[1:])
if step is None:
    check(False, "the prop window stepped down while the sprite was perched")
else:
    noticed = first(lambda f: f[0] >= step["at_ms"] and f[1] == "Falling")
    check(noticed is not None and noticed[0] - step["at_ms"] <= POLL_MS + SLACK_MS,
          "a window that moves is noticed within about one poll interval",
          f"{noticed[0] - step['at_ms']:.0f}ms" if noticed else "never noticed")
    check(first(lambda f: f[0] > step["at_ms"] and f[1] == "Perched"
                and abs(f[3] - step["y"]) <= 1) is not None,
          "the sprite follows the window to its new top edge",
          f"new top y={step['y']:.0f}")

dropped = first(lambda f: f[0] >= closed_ms and f[1] == "Falling")
check(dropped is not None and dropped[0] - closed_ms <= POLL_MS + SLACK_MS,
      "a window that closes drops the sprite within about one poll interval",
      f"{dropped[0] - closed_ms:.0f}ms" if dropped else "never dropped")

# At rest means not moving: the same position, frame after frame.
tail = frames[-10:]
check(all(f[2:] == tail[0][2:] for f in tail) and tail[-1][1] in ("Grounded", "Perched"),
      "and comes to rest again",
      f"{tail[-1][1]} at ({tail[-1][2]:.0f},{tail[-1][3]:.0f})")
check(tail[-1][3] > steps[0]["y"],
      "lower than the window it had been perched on")

# The usable floor, not the display's bottom edge. A screen reserves a strip of
# itself for the Dock, and the sprite rests on the near edge of it rather than
# behind it (#39). An inequality against the display bottom would pass either
# way and so would never notice the difference; this is an equality.
usable = [d["usable"] for d in displays]
floors = [u["y"] + u["h"] for u in usable
          if u["x"] <= tail[-1][2] <= u["x"] + u["w"]]
check(any(abs(tail[-1][3] - floor) <= 1 for floor in floors),
      "comes to rest on the usable floor rather than behind the Dock",
      "floors " + ", ".join(f"{floor:.0f}" for floor in floors))

# The Dock and the menu bar are not Perches. The prop at the Dock's own window
# level stood in the sprite's way on the first fall: it had to pass through.
over = next((json.loads(line) for line in open(f"{out}/over.log")
             if line.startswith("{")), None)
if over is None or over.get("layer", 0) == 0:
    check(False, "the elevated prop reported an elevated window level",
          f"layer={over.get('layer') if over else 'nothing reported'}")
else:
    check(first(lambda f: f[1] == "Perched" and abs(f[3] - over["y"]) <= 1) is None,
          "a window above the application level is not a Perch",
          f"layer {over['layer']:.0f} top y={over['y']:.0f}")
    check(first(lambda f: f[1] == "Falling" and f[3] > over["y"] + 1) is not None,
          "the sprite falls straight through it",
          "it was still falling below that edge")

# And the real furniture, whatever this desktop happens to have running: the
# menu bar, the Dock, the status items, Notification Centre.
furniture = json.load(open(f"{out}/desktop.json"))["elevated"]
stood_on = [w for w in furniture
            for f in frames
            if f[1] in ("Perched", "Grounded") and abs(f[3] - w["y"]) <= 1
            and w["x"] <= f[2] <= w["x"] + w["w"]]
check(not stood_on,
      "the desktop's own furniture is never stood on",
      f"{len(furniture)} elevated windows"
      if not stood_on else f"stood on {stood_on[0]['owner']}")

print(f"\n{'FAILED: ' + ', '.join(fails) if fails else 'Frame loop checks passed.'}")
sys.exit(1 if fails else 0)
PY
STATUS=$?

lsappinfo list 2> /dev/null | grep -A 4 '"ai-buddy"' > "$OUT/lsappinfo.txt"

# A tight crop around the sprite, with padding, so the edges of the art can be
# eyeballed against whatever is behind them. A full-desktop grab is too big to
# judge transparency from.
#
# Taken at rest rather than mid-run. It used to have to be mid-run: at rest the
# sprite sat at the bottom of the screen, where the Dock draws over it. Now it
# rests on the Dock (#39), so this crop is also the proof of that — the sprite
# is whole, above the Dock, rather than three quarters buried in it.
python3 - "$OUT" << 'PY'
import json, re, subprocess, sys

out = sys.argv[1]
log = open(f"{out}/app.log").read()
windows = json.load(open(f"{out}/window.json"))["windows"]
size = re.search(r"sprite (\d+)x(\d+)", log)
at = re.findall(r"^frame: \S+ Grounded \S+ sprite\((-?\d+),(-?\d+)\)", log, re.M)

if windows and size and at:
    width, height = (int(g) for g in size.groups())
    x, y = (int(g) for g in at[-1])
    pad = 40
    region = (f"{windows[0]['x'] + x - pad},{windows[0]['y'] + y - pad},"
              f"{width + pad * 2},{height + pad * 2}")
    subprocess.run(["screencapture", "-x", "-R", region, f"{out}/sprite.png"], check=False)
PY

echo "Capturing screenshots..."
DISPLAY_COUNT=$(python3 -c "import json;print(len(json.load(open('$OUT/window.json'))['displays']))" 2> /dev/null || echo 1)
for i in $(seq 1 "$DISPLAY_COUNT"); do
  screencapture -x -D "$i" "$OUT/display$i.png" 2> /dev/null ||
    echo "  (display $i capture failed - is Screen Recording granted to your terminal?)"
done

python3 - "$OUT" << 'PY'
import json, os, re, struct, sys

out = sys.argv[1]
data = json.load(open(f"{out}/window.json"))
displays, windows = data["displays"], data["windows"]
fails = []

def check(ok, label, detail=""):
    print(f"  {'PASS' if ok else 'FAIL'}  {label}{'  ' + detail if detail else ''}")
    if not ok:
        fails.append(label)

print("\nDisplays:")
for d in displays:
    print(f"  {d['w']:.0f}x{d['h']:.0f} at ({d['x']:.0f},{d['y']:.0f})")

print("\nChecks:")
check(len(windows) == 1, "exactly one overlay window", f"found {len(windows)}")
if not windows:
    sys.exit(1)

w = windows[0]
check(w["onscreen"], "window is on screen")
check(w["layer"] == 3, "floating window level", f"layer={w['layer']}")

# One display exactly, rather than the union of them: macOS gives each display
# its own Space and draws a window spanning two of them on only one, so an
# overlay wider than a display is invisible on every display but that one. The
# frame loop moves it to whichever display the Character is on, and the origin
# has to match too — a window covering the right area of the wrong display is
# the same disappearance.
#
# Any match, not exactly one: mirrored displays report the same rectangle, and
# two matches there is one answer said twice rather than a window spanning two
# screens. What rules a union out is the equality itself.
covered = [d for d in displays
           if (w["x"], w["y"], w["w"], w["h"]) == (d["x"], d["y"], d["w"], d["h"])]
check(bool(covered),
      "window covers one whole display and no more",
      f"{w['w']:.0f}x{w['h']:.0f} at ({w['x']:.0f},{w['y']:.0f})")

ls = open(f"{out}/lsappinfo.txt").read()
check('type="UIElement"' in ls, "accessory app: no Dock tile or switcher entry")

log = open(f"{out}/app.log").read()
check(re.search(r"^character: ", log, re.M) is not None, "app loaded a Character Package")
check(re.search(r"^overlay: ", log, re.M) is not None, "app reported its geometry on startup")
check("hit-test:" in log, "hit-test trace is running")
check("frame:" in log, "frame trace is running")

shots = [f for f in sorted(os.listdir(out)) if f.endswith(".png")]
for s in shots:
    d = open(f"{out}/{s}", "rb").read()
    pw, ph = struct.unpack(">II", d[16:24])
    print(f"  SHOT  {s}  {pw}x{ph}")
check(len(shots) > 0, "captured at least one screenshot")

print(f"\n{'FAILED: ' + ', '.join(fails) if fails else 'All machine checks passed.'}")
print(f"Artifacts: {out}")
sys.exit(1 if fails else 0)
PY
OVERLAY_STATUS=$?
[ "$OVERLAY_STATUS" -ne 0 ] && STATUS=1

# ---------------------------------------------------------------------------
# Hit-test pipeline, end to end, against the real art.
#
# The cursor is moved onto the resting sprite and off it again, which is the
# only end left to move now that the sprite's position belongs to the Engine.
# Two cases against the *same* sprite isolate the alpha lookup: its centre is
# drawn, its top-left corner is not. The cursor is put back where it was.
# ---------------------------------------------------------------------------
echo ""
echo "Hit-test pipeline:"

BEFORE=$(swift scripts/cursor-position.swift)
SPRITE_AT=$(
  python3 - "$OUT" << 'PY'
import json, re, sys
out = sys.argv[1]
log = open(f"{out}/app.log").read()
w = json.load(open(f"{out}/window.json"))["windows"][0]
size = re.search(r"sprite (\d+)x(\d+)", log)
at = re.findall(r"^frame: .* sprite\((-?\d+),(-?\d+)\)", log, re.M)
if not (size and at):
    sys.exit(1)
# Where the art's top-left corner is on screen, and how big it is.
print(int(w["x"]) + int(at[-1][0]), int(w["y"]) + int(at[-1][1]), *size.groups())
PY
) || SPRITE_AT=""

probe() { # $1=offset into the art  $2=label  $3=expected HIT|miss
  local x y line landed
  x=$(($(echo "$SPRITE_AT" | cut -d' ' -f1) + $1))
  y=$(($(echo "$SPRITE_AT" | cut -d' ' -f2) + $1))

  landed=$(swift scripts/warp-cursor.swift "$x" "$y")
  if [ "$landed" != "$x $y" ]; then
    echo "  SKIP  $2 - the cursor could not be placed at $x $y (landed at $landed)"
    return
  fi
  # The decision is made on the next tick, not on the warp.
  perl -e 'select(undef,undef,undef,0.5)'

  line=$(grep 'hit-test:' "$OUT/app.log" | tail -1)
  if echo "$line" | grep -q "$3 "; then
    echo "  PASS  $2"
  else
    echo "  FAIL  $2 (expected $3)"
    echo "        $line"
    HIT_FAILED=1
  fi
}

HIT_FAILED=0
if [ -z "$SPRITE_AT" ]; then
  echo "  SKIP  the app never reported where the sprite is"
else
  # 32x32 art at scale 4 is 128 points. Half of that puts the cursor at the
  # sprite's centre, which is drawn; offset 0 is its top-left corner, which is
  # not.
  probe $(($(echo "$SPRITE_AT" | cut -d' ' -f3) / 2)) "cursor over drawn pixels swallows clicks" "HIT"
  probe 0 "cursor over transparent pixels passes clicks through" "miss"
  # Put the cursor back where the human left it.
  # shellcheck disable=SC2086  # an x and a y, deliberately split
  swift scripts/warp-cursor.swift $BEFORE > /dev/null
fi

[ "$HIT_FAILED" = "1" ] && STATUS=1

echo ""
echo "Still needs a human (README > Verifying the overlay by hand):"
echo "  that the window server honours the flag - a click really lands underneath,"
echo "  and typing elsewhere really survives a click on the sprite."

if [ "$KEEP" = "1" ]; then
  printf '\n'
  echo "App still running (pid $APP_PID) for the manual checks, with tracing on."
  echo "Watch the decisions:  tail -f $OUT/app.log"
  echo "Stop it:              pkill -f target/debug/ai-buddy"
fi
exit $STATUS
