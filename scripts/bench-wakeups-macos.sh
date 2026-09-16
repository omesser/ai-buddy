#!/usr/bin/env bash
# Baseline: wakeups/sec, package idle residency, and CPU time for a release
# ai-buddy binary, per scenario. A reviewer reruns this directly rather than
# trusting a number in a doc.

# Needs: sudo (powermetrics), swift (click-cursor.swift, chat scenario only),
# AI_BUDDY_TRACE_FRAMES so the frame log proves what the sprite was doing, and
# AI_BUDDY_DIRECTOR_API_KEY so a worktree build does not block on a Keychain prompt.
# Usage:
#   scripts/bench-wakeups-macos.sh --binary PATH --scenario idle|chat \
#     [--duration SECS] [--out DIR]

# idle: launch, settle, sample. Ambient Behaviors may fire on their own; the
#   frame log lets the parser split the sample by what was on screen.
# chat: launch, double-click the sprite's centre from the frame log with
#   scripts/click-cursor.swift (shared with crates/verify; do not fork it),
#   confirm the Summon verb, then sample with chat open.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

BINARY=""
SCENARIO=""
DURATION=30
OUT=".verify/bench-$(date +%Y%m%d-%H%M%S)"

while [ $# -gt 0 ]; do
  case "$1" in
    --binary)
      BINARY="$2"
      shift 2
      ;;
    --scenario)
      SCENARIO="$2"
      shift 2
      ;;
    --duration)
      DURATION="$2"
      shift 2
      ;;
    --out)
      OUT="$2"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

[ -x "$BINARY" ] || {
  echo "bench-wakeups-macos: --binary $BINARY not executable" >&2
  exit 2
}
case "$SCENARIO" in idle | chat) ;; *)
  echo "bench-wakeups-macos: --scenario must be idle or chat" >&2
  exit 2
  ;;
esac

mkdir -p "$OUT"
LOG="$OUT/app.log"
PM="$OUT/powermetrics.txt"
META="$OUT/meta.txt"

trap 'kill "$APP_PID" 2> /dev/null; wait "$APP_PID" 2> /dev/null' EXIT INT TERM

AI_BUDDY_DIRECTOR_API_KEY=bench-placeholder \
  AI_BUDDY_DIRECTOR=0 \
  AI_BUDDY_TRACE_FRAMES=1 \
  AI_BUDDY_TRACE_ENGINE=1 \
  AI_BUDDY_CAPTURABLE=1 \
  "$BINARY" > "$LOG" 2>&1 &
APP_PID=$!

echo "launched pid=$APP_PID binary=$BINARY log=$LOG" >&2

READY=0
for _ in $(seq 1 40); do
  grep -q '^overlay:' "$LOG" 2> /dev/null && {
    READY=1
    break
  }
  kill -0 "$APP_PID" 2> /dev/null || break
  perl -e 'select(undef,undef,undef,0.25)'
done
if [ "$READY" -ne 1 ]; then
  echo "bench-wakeups-macos: app never reported '^overlay:' - see $LOG" >&2
  exit 1
fi

# Let the first ticks settle before driving Summon or starting the idle clock:
# the sprite spawns mid-air and plays Falling until it lands, and a click during
# that window can miss the moving art, so wait for Grounded.
for _ in $(seq 1 40); do
  grep -qE '^frame: .* (Grounded|Perched) ' "$LOG" 2> /dev/null && break
  perl -e 'select(undef,undef,undef,0.25)'
done
sleep 1

SUMMON_STATUS="n/a"
if [ "$SCENARIO" = "chat" ]; then
  SPRITE_AT=$(
    python3 - "$LOG" << 'PY'
import re, sys
log = open(sys.argv[1]).read()
size = re.search(r"sprite (\d+)x(\d+)", log)
at = re.findall(r"^frame: .* (?:Grounded|Perched) .*? sprite\((-?\d+),(-?\d+)\)", log, re.M)
if not (size and at):
    sys.exit(1)
print(int(at[-1][0]), int(at[-1][1]), *size.groups())
PY
  )
  if [ -z "$SPRITE_AT" ]; then
    echo "bench-wakeups-macos: could not read sprite position for chat scenario" >&2
    exit 1
  fi
  read -r SX SY SW SH <<< "$SPRITE_AT"
  CX=$((SX + SW / 2))
  CY=$((SY + SH / 2))
  # `osascript ... click at` resolves an AX element under the point first, and
  # this borderless panel presents none (fails -25208). click-cursor.swift warps
  # the cursor and posts real HID events instead. `2` clicks reads as one Summon.
  swift scripts/click-cursor.swift "$CX" "$CY" 2 > "$OUT/click.log" 2>&1
  sleep 1
  if grep -qE 'verbs:.*Summon' "$LOG"; then
    SUMMON_STATUS="confirmed"
  else
    SUMMON_STATUS="NOT CONFIRMED - see $OUT/click.log and $LOG"
    echo "bench-wakeups-macos: $SUMMON_STATUS" >&2
  fi
  sleep 2
fi

START_ISO=$(date -u +%Y-%m-%dT%H:%M:%SZ)
sudo powermetrics -i 1000 -n "$DURATION" --samplers tasks,cpu_power -o "$PM"
PM_STATUS=$?
END_ISO=$(date -u +%Y-%m-%dT%H:%M:%SZ)

{
  echo "scenario: $SCENARIO"
  echo "binary: $BINARY"
  echo "binary_mtime: $(stat -f '%Sm' "$BINARY")"
  echo "duration_secs: $DURATION"
  echo "start: $START_ISO"
  echo "end: $END_ISO"
  echo "pid: $APP_PID"
  echo "note: powermetrics is system-wide - another agent's ai-buddy build"
  echo "  can be running at the same time. Always parse with --pid $APP_PID,"
  echo "  never bare --process name matching."
  echo "summon: $SUMMON_STATUS"
  echo "powermetrics_exit: $PM_STATUS"
  echo "machine: $(sysctl -n hw.model)"
  echo "os: $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
  echo "git_rev: $(git rev-parse --short HEAD 2> /dev/null || echo unknown)"
} > "$META"

cat "$META"
[ "$PM_STATUS" -eq 0 ] || exit 1
