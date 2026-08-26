#!/usr/bin/env bash
#
# Machine-checkable verification for the overlay window.
#
# Deliberately not a cargo test: every check here needs a real desktop, a real
# window server and a running app, so it is slow, it is macOS-only, and it
# cannot run in CI. Run it by hand when the overlay or the platform layer
# changes. `cargo test` stays fast and pure.
#
# What this cannot check: whether a click actually passes through to the window
# underneath, and whether typing elsewhere survives a click on the sprite. Those
# need a human. See the checklist in README.md.
#
# Usage: scripts/verify-overlay.sh [--keep]
#   --keep   leave the app running afterwards, with hit-test tracing on

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1

STAMP=$(date +%Y%m%d-%H%M%S)
OUT=".verify/$STAMP"
mkdir -p "$OUT"

echo "Building..."
if ! cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -3; then
  echo "FAIL: build"
  exit 1
fi

pkill -f 'target/debug/ai-buddy' 2> /dev/null
AI_BUDDY_TRACE_HITTEST=1 ./src-tauri/target/debug/ai-buddy > "$OUT/app.log" 2>&1 &
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

swift scripts/inspect-window.swift > "$OUT/window.json" 2> "$OUT/window.err" ||
  {
    echo "FAIL: inspector"
    cat "$OUT/window.err"
  }

lsappinfo list 2> /dev/null | grep -A 4 '"ai-buddy"' > "$OUT/lsappinfo.txt"

echo "Capturing screenshots..."
DISPLAY_COUNT=$(python3 -c "import json;print(len(json.load(open('$OUT/window.json'))['displays']))" 2> /dev/null || echo 1)
for i in $(seq 1 "$DISPLAY_COUNT"); do
  screencapture -x -D "$i" "$OUT/display$i.png" 2> /dev/null ||
    echo "  (display $i capture failed - is Screen Recording granted to your terminal?)"
done

python3 - "$OUT" << 'PY'
import json, os, re, struct, subprocess, sys

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

union_l = min(d["x"] for d in displays)
union_t = min(d["y"] for d in displays)
union_r = max(d["x"] + d["w"] for d in displays)
union_b = max(d["y"] + d["h"] for d in displays)
union_w, union_h = union_r - union_l, union_b - union_t

print("\nChecks:")
check(len(windows) == 1, "exactly one overlay window", f"found {len(windows)}")
if not windows:
    sys.exit(1)

w = windows[0]
check(w["onscreen"], "window is on screen")
check(w["layer"] == 3, "floating window level", f"layer={w['layer']}")
check(w["w"] == union_w and w["h"] == union_h,
      "window spans the display union",
      f"{w['w']:.0f}x{w['h']:.0f} vs {union_w:.0f}x{union_h:.0f}")

# Known and deferred to #4, so reported but not failed.
dx, dy = w["x"] - union_l, w["y"] - union_t
if dx or dy:
    print(f"  NOTE  origin offset by ({dx:.0f},{dy:.0f}) - known, see #4")

ls = open(f"{out}/lsappinfo.txt").read()
check('type="UIElement"' in ls, "accessory app: no Dock tile or switcher entry")

log = open(f"{out}/app.log").read()
check(log.startswith("overlay:"), "app reported its geometry on startup")
check("hit-test:" in log, "hit-test trace is running")

# A tight crop around the sprite, with padding, so the edges of the art can be
# eyeballed against whatever is behind them. A full-desktop grab is too big to
# judge transparency from.
m = re.search(r"sprite (\d+)x(\d+) at \((-?\d+),(-?\d+)\)", log)
if m and windows:
    sw, sh, sx, sy = (int(g) for g in m.groups())
    pad = 40
    region = f"{w['x'] + sx - pad},{w['y'] + sy - pad},{sw + pad * 2},{sh + pad * 2}"
    subprocess.run(["screencapture", "-x", "-R", region, f"{out}/sprite.png"], check=False)

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
STATUS=$?

# ---------------------------------------------------------------------------
# Hit-test pipeline, end to end, against the real art.
#
# The sprite is moved onto wherever the cursor already is, rather than moving
# the cursor, which would need Accessibility and synthetic events. Two cases at
# the *same* cursor position isolate the alpha lookup: the sprite's centre is
# drawn, its top-left corner is not.
# ---------------------------------------------------------------------------
echo ""
echo "Hit-test pipeline:"

CURSOR_BEFORE=$(swift scripts/cursor-position.swift)
CX=$(echo "$CURSOR_BEFORE" | cut -d' ' -f1)
CY=$(echo "$CURSOR_BEFORE" | cut -d' ' -f2)
WIN_Y=$(python3 -c "import json;print(int(json.load(open('$OUT/window.json'))['windows'][0]['y']))" 2> /dev/null || echo 0)
WIN_X=$(python3 -c "import json;print(int(json.load(open('$OUT/window.json'))['windows'][0]['x']))" 2> /dev/null || echo 0)

# Cursor in window-local points, which is where the sprite is placed.
LOCAL_X=$((CX - WIN_X))
LOCAL_Y=$((CY - WIN_Y))

probe() { # $1=sprite pos  $2=label  $3=expected HIT|miss
  pkill -f 'target/debug/ai-buddy' 2> /dev/null
  AI_BUDDY_TRACE_HITTEST=1 AI_BUDDY_SPRITE_POS="$1" ./src-tauri/target/debug/ai-buddy > "$OUT/probe-$3.log" 2>&1 &
  local pid=$!
  for _ in $(seq 1 40); do
    grep -q 'hit-test:' "$OUT/probe-$3.log" 2> /dev/null && break
    perl -e 'select(undef,undef,undef,0.25)'
  done
  # The first trace line fires before the window frame settles: applying the
  # non-activating style mask resizes it, and until that lands the window
  # reports a different scale factor and origin. Sample after it settles, not
  # at first sight of output.
  perl -e 'select(undef,undef,undef,2.5)'
  local line
  line=$(grep 'hit-test:' "$OUT/probe-$3.log" | tail -1)
  kill "$pid" 2> /dev/null
  if echo "$line" | grep -q "$3 "; then
    echo "  PASS  $2"
  else
    echo "  FAIL  $2 (expected $3)"
    echo "        $line"
    HIT_FAILED=1
  fi
}

HIT_FAILED=0
# 32x32 art at scale 4 is 128 points; half of that centres it on the cursor.
probe "$((LOCAL_X - 64)),$((LOCAL_Y - 64))" "cursor over drawn pixels swallows clicks" "HIT"
probe "$LOCAL_X,$LOCAL_Y" "cursor over transparent pixels passes clicks through" "miss"

CURSOR_AFTER=$(swift scripts/cursor-position.swift)
if [ "$CURSOR_BEFORE" != "$CURSOR_AFTER" ]; then
  echo "  NOTE  cursor moved during the probes ($CURSOR_BEFORE -> $CURSOR_AFTER);"
  echo "        rerun without touching the mouse if either case failed."
fi
[ "$HIT_FAILED" = "1" ] && STATUS=1

echo ""
echo "Still needs a human (README > Verifying the overlay by hand):"
echo "  that the window server honours the flag - a click really lands underneath,"
echo "  and typing elsewhere really survives a click on the sprite."

if [ "$KEEP" = "1" ]; then
  printf '\n'
  echo "App left running (pid $APP_PID) with tracing on. Move the cursor over the"
  echo "sprite, then: tail -f $OUT/app.log"
else
  kill "$APP_PID" 2> /dev/null
fi
exit $STATUS
