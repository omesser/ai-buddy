#!/usr/bin/env bash
# Display-frame cadence, dropped frames and interpolation lag on macOS (#426).
# Launches fidget with FIDGET_TRACE_FRAMES (Engine ticks) and
# FIDGET_TRACE_CADENCE (the overlay's display frames), samples a window, and
# reduces it with scripts/frame-cadence.mjs into one report per scenario.
#
# idle: the sprite left alone. walking: sampled from the first walk frame.
# load: walking with one `yes` per core running from launch.
# idle-quiet: idle with FIDGET_TRACE_FRAMES off, so the loop's own tick
#   counter says whether the per-tick `frame:` print slows the Engine.
# Every scenario launches fidget on the live desktop, so it refuses to run
# unless FIDGET_BENCH_GREEN_LIGHT=1 says the operator agreed to it.

set -euo pipefail
cd "$(dirname "$0")/.."

seconds=20
walk_timeout=180
out=""
bin="${FIDGET_VERIFY_BIN:-target/debug/fidget}"
scenario=""

usage() {
  cat >&2 << EOF
Usage: $0 <idle|idle-quiet|walking|load|matrix> [--seconds N] [--walk-timeout N] [--bin PATH] [--out DIR]

Every scenario launches fidget and needs FIDGET_BENCH_GREEN_LIGHT=1.
EOF
  exit 2
}

while [ $# -gt 0 ]; do
  case "$1" in
    --seconds)
      seconds="$2"
      shift 2
      ;;
    --walk-timeout)
      walk_timeout="$2"
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
    idle | idle-quiet | walking | load | matrix)
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
if [ "${FIDGET_BENCH_GREEN_LIGHT:-}" != 1 ]; then
  echo "$scenario launches fidget on the live desktop. Set FIDGET_BENCH_GREEN_LIGHT=1 once the operator has agreed." >&2
  exit 2
fi
[ -x "$bin" ] || {
  echo "no $bin; run: (cd src-tauri && cargo build --bin fidget)" >&2
  exit 2
}

out="${out:-$(mktemp -d /tmp/fidget-cadence-bench-XXXXXX)}"
mkdir -p "$out"

APP_PID=""
SCRATCH_HOME=""
BURNERS=()

stop_app() {
  if [ -n "$APP_PID" ]; then
    kill -TERM "$APP_PID" 2> /dev/null || true
    sleep 0.3
    kill -KILL "$APP_PID" 2> /dev/null || true
    wait "$APP_PID" 2> /dev/null || true
  fi
  APP_PID=""
  [ -z "$SCRATCH_HOME" ] || rm -rf "$SCRATCH_HOME"
  SCRATCH_HOME=""
}

stop_burners() {
  [ "${#BURNERS[@]}" -eq 0 ] || kill "${BURNERS[@]}" 2> /dev/null || true
  BURNERS=()
}

trap 'stop_app; stop_burners' EXIT

now_ms() {
  perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000'
}

launch_app() {
  local log=$1 frames=$2
  # Scratch HOME so the bench does not write the user's settings. The API key
  # skips the Keychain read a worktree build would otherwise block on.
  SCRATCH_HOME=$(mktemp -d)
  FIDGET_DIRECTOR_API_KEY=bench-placeholder \
    FIDGET_DIRECTOR=0 \
    FIDGET_TRACE_FRAMES="$frames" \
    FIDGET_TRACE_CADENCE=1 \
    FIDGET_INSTANCES="${FIDGET_INSTANCES:-BMO}" \
    FIDGET_CHARACTERS="${FIDGET_CHARACTERS:-$PWD/characters}" \
    HOME="$SCRATCH_HOME" \
    "$bin" > "$log" 2>&1 &
  APP_PID=$!
}

# The sprite spawns mid-air; wait for it to land so a fall is not sampled.
wait_landed() {
  local log=$1 _
  for _ in $(seq 1 80); do
    grep -qE '^frame: .* (Grounded|Perched) ' "$log" 2> /dev/null && return 0
    kill -0 "$APP_PID" 2> /dev/null || return 1
    sleep 0.25
  done
  return 1
}

wait_walk() {
  local log=$1 deadline=$((SECONDS + walk_timeout))
  while [ "$SECONDS" -lt "$deadline" ]; do
    grep -qE ' (walk|ballwalk)#' "$log" && return 0
    kill -0 "$APP_PID" 2> /dev/null || return 1
    sleep 0.5
  done
  return 1
}

run() {
  local name=$1 log="$out/$1.log"
  if [ "$name" = load ]; then
    for _ in $(seq 1 "$(sysctl -n hw.ncpu)"); do
      yes > /dev/null &
      BURNERS+=($!)
    done
  fi
  if [ "$name" = idle-quiet ]; then
    launch_app "$log" 0
    # No `frame:` lines to watch for a landing, so give the fall a fixed 5 s.
    sleep 5
  else
    launch_app "$log" 1
  fi
  if [ "$name" != idle-quiet ] && ! wait_landed "$log"; then
    echo "$name: the sprite never landed; see $log" >&2
    stop_app
    stop_burners
    return 1
  fi
  sleep 2
  if [ "${name%-quiet}" != idle ] && ! wait_walk "$log"; then
    echo "$name: no walk frame within ${walk_timeout}s; see $log" >&2
    stop_app
    stop_burners
    return 1
  fi
  local from to
  from=$(now_ms)
  sleep "$seconds"
  to=$(now_ms)
  # The overlay sends its frames once a second, so the last batch is still in flight.
  sleep 1.5
  stop_app
  stop_burners
  {
    echo "## $name"
    echo
    echo "window: $from..$to ms, walk frames: $(awk -v f="$from" -v t="$to" '/^frame: / && $2 >= f && $2 <= t && / (walk|ballwalk)#/ { n++ } END { print n + 0 }' "$log")"
    echo
    node scripts/frame-cadence.mjs "$log" --from "$from" --to "$to"
  } > "$out/$name.md"
  cat "$out/$name.md"
  echo
}

cat << EOF
machine=$(sysctl -n hw.model)
os=$(sw_vers -productVersion) ($(sw_vers -buildVersion))
refresh_hz=$(system_profiler SPDisplaysDataType 2> /dev/null | sed -n 's/.*@ \([0-9.]*\)Hz.*/\1/p' | tr '\n' ' ')
cpus=$(sysctl -n hw.ncpu)
bin=$bin
git_rev=$(git rev-parse --short HEAD 2> /dev/null || echo unknown)
seconds=$seconds
EOF
echo

case "$scenario" in
  matrix)
    run idle
    run idle-quiet
    run walking
    run load
    ;;
  *) run "$scenario" ;;
esac

echo "out=$out"
