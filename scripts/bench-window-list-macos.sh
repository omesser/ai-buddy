#!/usr/bin/env bash
# What Fidget's window-list poll costs on macOS (#427): how often the frame
# loop calls CGWindowListCopyWindowInfo idle (10 Hz) and riding (60 Hz), what
# each call takes, and how that grows with the window count. One TSV row per
# scenario, the same shape as bench-gpu-compositing-macos.sh.
#
# micro times the call from its own process against whatever is on the desktop
# (scripts/bench-window-list-macos.swift) and needs no green light. idle and
# riding launch Fidget on the live desktop and flood it with windows, so they
# refuse to run unless FIDGET_BENCH_GREEN_LIGHT=1 says the operator agreed.
# The flood comes from scripts/window-flood-macos.swift (PR #1043); without
# that file the added-window rows skip and say so.
#
# The per-call numbers for a running Fidget come from dtrace on SkyLight's
# SLWindowListCopyWindowInfo: on macOS 26 CoreGraphics forwards to it and the
# pid provider has no CGWindowListCopyWindowInfo probe to offer. That needs
# sudo; run `sudo -v` first for an unattended run.

set -euo pipefail
cd "$(dirname "$0")/.."

seconds=15
windows=100
out=""
bin="${FIDGET_VERIFY_BIN:-target/debug/fidget}"
scenario=""

usage() {
  cat >&2 << EOF
Usage: $0 <env|micro|idle|riding|matrix> [--seconds N] [--windows N] [--bin PATH] [--out DIR]

env and micro touch nothing on screen. idle and riding launch Fidget and open
windows, and need FIDGET_BENCH_GREEN_LIGHT=1. matrix runs idle and riding
twice each: on the desktop as found, and with --windows flood windows added by
scripts/window-flood-macos.swift, skipped when that file is absent. Per-call
timing of the running app needs sudo for dtrace.
EOF
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    --seconds)
      seconds="$2"
      shift 2
      ;;
    --windows)
      windows="$2"
      shift 2
      ;;
    --bin)
      bin="$2"
      shift 2
      ;;
    --out)
      out="$2"
      shift 2
      ;;
    --help | -h) usage ;;
    env | micro | idle | riding | matrix)
      scenario="$1"
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      ;;
  esac
done

[ -n "$scenario" ] || usage

case "$scenario" in
  env | micro) ;;
  *)
    if [ "${FIDGET_BENCH_GREEN_LIGHT:-}" != 1 ]; then
      echo "$scenario takes over the desktop (launches Fidget, opens windows)." >&2
      echo "Set FIDGET_BENCH_GREEN_LIGHT=1 once the operator has agreed." >&2
      exit 2
    fi
    ;;
esac

out="${out:-$(mktemp -d /tmp/fidget-window-list-bench-XXXXXX)}"
mkdir -p "$out"

APP_PID=""
PROP_PIDS=""
SCRATCH_HOME=""

cleanup() {
  stop_app
  stop_props
}
trap cleanup EXIT

stop_app() {
  if [ -n "$APP_PID" ]; then
    kill -TERM "$APP_PID" 2> /dev/null || true
    sleep 0.3
    kill -KILL "$APP_PID" 2> /dev/null || true
    wait "$APP_PID" 2> /dev/null || true
  fi
  APP_PID=""
  if [ -n "$SCRATCH_HOME" ]; then
    rm -rf "$SCRATCH_HOME"
  fi
  SCRATCH_HOME=""
}

stop_props() {
  local pid
  for pid in $PROP_PIDS; do
    kill -TERM "$pid" 2> /dev/null || true
  done
  PROP_PIDS=""
}

need_bin() {
  [ -x "$bin" ] || {
    echo "no $bin — run: (cd src-tauri && cargo build --bin fidget)" >&2
    exit 2
  }
}

probe_host() {
  MACHINE=$(sysctl -n hw.model)
  OS="$(sw_vers -productVersion) ($(sw_vers -buildVersion))"
  SUDO_OK=0
  if sudo -n true 2> /dev/null; then
    SUDO_OK=1
  elif [ "$scenario" != env ] && [ "$scenario" != micro ] && [ -t 0 ] && sudo -v; then
    SUDO_OK=1
  fi
}

print_env() {
  cat << EOF
machine=$MACHINE
os=$OS
sudo_cached=$SUDO_OK
bin=$bin
seconds=$seconds
windows=$windows
probe=pid\$target:SkyLight:SLWindowListCopyWindowInfo (CGWindowListCopyWindowInfo has no pid probe here)
green_light=${FIDGET_BENCH_GREEN_LIGHT:-unset}
EOF
}

emit() {
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$@" | tee -a "$out/rows.tsv"
}

# The on-screen window count as the app's own options see it, from the
# microbench's `app-call` row. The app does not log it on macOS.
on_screen_count() {
  swift scripts/bench-window-list-macos.swift --iterations 5 --warmup 1 2> /dev/null |
    awk -F'\t' '$1 == "app-call" { print $2 }'
}

run_micro() {
  local tsv="$out/micro.tsv"
  swift scripts/bench-window-list-macos.swift --iterations 300 > "$tsv"
  awk -F'\t' 'NR > 1 { print }' "$tsv" | while IFS=$'\t' read -r mode count iters median p95 max per; do
    emit "micro-$mode" "$count" N/A "$median" "$p95" "$max" "in-process, $iters iterations, ${per}us/window"
  done
}

# Fills POLLS_HZ, MEDIAN_US, P95_US, MAX_US, POLL_REASON from one dtrace
# sample of the running app. One line per call, reduced here.
sample_polls() {
  local name=$1
  local raw="$out/${name}.calls.txt"
  POLLS_HZ=N/A
  MEDIAN_US=N/A
  P95_US=N/A
  MAX_US=N/A
  if [ "$SUDO_OK" -ne 1 ]; then
    POLL_REASON="dtrace skipped (sudo not cached)"
    return 0
  fi
  sudo -n dtrace -q -p "$APP_PID" -o "$raw" -n '
    pid$target:SkyLight:SLWindowListCopyWindowInfo:entry { self->t = timestamp; }
    pid$target:SkyLight:SLWindowListCopyWindowInfo:return /self->t/ {
      printf("%d\n", (timestamp - self->t) / 1000); self->t = 0;
    }
    tick-'"$seconds"'s { exit(0); }' 2> "$out/${name}.dtrace.err" || true
  local n
  n=$(grep -c '^[0-9]' "$raw" 2> /dev/null || true)
  if [ "${n:-0}" -eq 0 ]; then
    POLL_REASON="dtrace saw no calls, see $out/${name}.dtrace.err"
    return 0
  fi
  read -r POLLS_HZ MEDIAN_US P95_US MAX_US < <(
    grep '^[0-9]' "$raw" | sort -n | awk -v s="$seconds" '
      { v[NR] = $1 }
      END {
        printf "%.2f %d %d %d\n", NR / s, v[int((NR + 1) / 2)], v[int(NR * 0.95) < 1 ? 1 : int(NR * 0.95)], v[NR]
      }'
  )
  POLL_REASON="dtrace $n calls over ${seconds}s"
}

launch_app() {
  local log=$1
  need_bin
  # Scratch HOME so the bench does not write the user's settings. The API key
  # skips the Keychain read a worktree build would otherwise block on.
  SCRATCH_HOME=$(mktemp -d)
  FIDGET_DIRECTOR_API_KEY=bench-placeholder \
    FIDGET_DIRECTOR=0 \
    FIDGET_TRACE_FRAMES=1 \
    FIDGET_INSTANCES="${FIDGET_INSTANCES:-BMO}" \
    FIDGET_CHARACTERS="${FIDGET_CHARACTERS:-$PWD/characters}" \
    HOME="$SCRATCH_HOME" \
    "$bin" > "$log" 2>&1 &
  APP_PID=$!
}

# Waits for the overlay, then for the sprite to settle on the given state.
wait_state() {
  local log=$1
  local pattern=$2
  local _
  for _ in $(seq 1 40); do
    if grep -q '^overlay: [0-9]* display' "$log" 2> /dev/null; then
      break
    fi
    if ! kill -0 "$APP_PID" 2> /dev/null; then
      return 1
    fi
    sleep 0.5
  done
  grep -q '^overlay: [0-9]* display' "$log" 2> /dev/null || return 1
  for _ in $(seq 1 60); do
    if grep -qE "^frame: [0-9]+ $pattern " "$log" 2> /dev/null; then
      return 0
    fi
    sleep 0.25
  done
  return 1
}

flood=scripts/window-flood-macos.swift

# Floods the display with $count windows and appends the flood's pid to
# PROP_PIDS. A count of 0 adds nothing, which is the "desktop as found" row.
# On failure ADD_REASON says why, so the row records it.
add_flood_windows() {
  local count=$1
  local log=$2
  ADD_REASON=""
  [ "$count" -gt 0 ] || return 0
  if [ ! -f "$flood" ]; then
    ADD_REASON="skipped: $flood is not in this checkout (under review in #1043)"
    return 1
  fi
  swift "$flood" "$count" $((seconds + 120)) > "$log" 2>&1 &
  PROP_PIDS="$PROP_PIDS $!"
  local _
  for _ in $(seq 1 40); do
    if grep -q '^{' "$log" 2> /dev/null; then
      return 0
    fi
    sleep 0.25
  done
  ADD_REASON="window flood never reported, see $log"
  return 1
}

# The perch rectangle, computed the way scripts/verify-overlay.sh does: the
# sprite spawns at the middle of the first display's usable frame and falls,
# so a wide window halfway to the floor is what it lands on.
perch_rect() {
  swift scripts/inspect-window.swift > "$out/desktop.json" 2> "$out/desktop.err" || return 1
  python3 - "$out/desktop.json" << 'PY'
import json, sys
u = json.load(open(sys.argv[1]))["displays"][0]["usable"]
sprite_x = u["x"] + u["w"] / 2
sprite_y = u["y"] + u["h"] / 2
width = min(1200.0, u["w"])
print(int(sprite_x - width / 2), int(sprite_y + u["h"] / 4), int(width), 240)
PY
}

run_idle() {
  local extra=$1
  local name="idle+$extra"
  local log="$out/$name.log"
  add_flood_windows "$extra" "$out/$name.flood.log" || {
    emit "$name" N/A N/A N/A N/A N/A "$ADD_REASON"
    stop_props
    return 0
  }
  launch_app "$log"
  if ! wait_state "$log" '(Grounded|Perched)'; then
    emit "$name" N/A N/A N/A N/A N/A "no Grounded or Perched frame; see $log"
    stop_app
    stop_props
    return 0
  fi
  sleep 2
  local count
  count=$(on_screen_count)
  sample_polls "$name"
  emit "$name" "$count" "$POLLS_HZ" "$MEDIAN_US" "$P95_US" "$MAX_US" "pointer left alone. $POLL_REASON"
  stop_app
  stop_props
}

run_riding() {
  local extra=$1
  local name="riding+$extra"
  local log="$out/$name.log"
  add_flood_windows "$extra" "$out/$name.flood.log" || {
    emit "$name" N/A N/A N/A N/A N/A "$ADD_REASON"
    stop_props
    return 0
  }
  local rect
  if ! rect=$(perch_rect); then
    emit "$name" N/A N/A N/A N/A N/A "inspect-window failed, see $out/desktop.err"
    stop_props
    return 0
  fi
  # The perch has to exist before the app does: the sprite falls in under a
  # second and a window that arrives later is above it.
  # shellcheck disable=SC2086  # four separate arguments, deliberately
  swift scripts/perch-window.swift --glide $rect > "$out/$name.perch.log" 2>&1 &
  PROP_PIDS="$PROP_PIDS $!"
  local _
  for _ in $(seq 1 40); do
    grep -q '^{' "$out/$name.perch.log" 2> /dev/null && break
    sleep 0.25
  done
  launch_app "$log"
  if ! wait_state "$log" 'Perched'; then
    emit "$name" N/A N/A N/A N/A N/A "the sprite never perched on the gliding prop; see $log"
    stop_app
    stop_props
    return 0
  fi
  sleep 2
  local from count
  from=$(awk 'END { print NR + 0 }' "$log")
  count=$(on_screen_count)
  sample_polls "$name"
  # Distinct sprite positions during the sample: a ride moves the sprite, a
  # perch that lost its window leaves one position.
  local moved
  moved=$(awk -v from="$from" 'NR > from && /^frame: [0-9]+ Perched / { split($0, a, "pos"); p[a[2]] = 1 } END { print length(p) }' "$log")
  emit "$name" "$count" "$POLLS_HZ" "$MEDIAN_US" "$P95_US" "$MAX_US" "gliding perch, $moved distinct perched positions. $POLL_REASON"
  stop_app
  stop_props
}

probe_host
echo "scenario	windows	polls_hz	median_us	p95_us	max_us	notes" > "$out/rows.tsv"

print_env
case "$scenario" in
  env) ;;
  micro) run_micro ;;
  idle) run_idle "$windows" ;;
  riding) run_riding "$windows" ;;
  matrix)
    run_micro
    run_idle 0
    run_idle "$windows"
    run_riding 0
    run_riding "$windows"
    ;;
esac

echo "out=$out"
column -t -s $'\t' "$out/rows.tsv" || cat "$out/rows.tsv"
