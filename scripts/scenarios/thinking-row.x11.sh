#!/usr/bin/env bash
# Scenario: thinking-row (X11)
# On screen: launches Fidget as BMO with a fixture Harness. Chat opens by
#   itself and takes focus. One AT-SPI dump of the Chat window while the
#   Harness thinks, and one after the reply. Fidget quits when the scenario ends.
# Input: none.
# Duration: about 30 s, 2 min at most.
# Grants: a running X11 session. AT-SPI (python3-pyatspi) for the terminal that runs it.
# Asserts: the Thinking row is open while the Harness thinks, and collapsed
#   once the reply lands.
#
# Usage: thinking-row.x11.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
# Set FIDGET_SCENARIO_AX_OPEN and FIDGET_SCENARIO_AX_DONE to assert those dumps
# and skip the GUI. That checks the assertion, not the live window.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0" || true
  exit 2
fi
bin=${2:?usage: thinking-row.x11.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: thinking-row.x11.sh --go <fidget binary> <fidget test binary>}
root=$(cd "$(dirname "$0")/../.." && pwd)

fail() {
  echo "FAIL: $*" >&2
  if [ -n "${out:-}" ]; then
    echo "evidence in $out" >&2
  fi
  exit 1
}

assert_row() { # <file> <glyph> <label>
  local file=$1 glyph=$2 label=$3 row lower prefix
  row=$(grep -iE '^button\|.*thinking' "$file" | head -n 1 || true)
  [ -n "$row" ] || fail "$label: no Thinking row in $file"
  lower=$(printf '%s' "$row" | tr '[:upper:]' '[:lower:]')
  prefix=$(printf 'button|%s' "$glyph" | tr '[:upper:]' '[:lower:]')
  case $lower in
    "$prefix"*) echo "ok: $label: $row" ;;
    *) fail "$label: Thinking row reads '$row', want it to start with $glyph" ;;
  esac
}

if [ -n "${FIDGET_SCENARIO_AX_OPEN:-}" ] || [ -n "${FIDGET_SCENARIO_AX_DONE:-}" ]; then
  [ -n "${FIDGET_SCENARIO_AX_OPEN:-}" ] && [ -n "${FIDGET_SCENARIO_AX_DONE:-}" ] \
    || fail "set both FIDGET_SCENARIO_AX_OPEN and FIDGET_SCENARIO_AX_DONE"
  assert_row "$FIDGET_SCENARIO_AX_OPEN" "▾" "open"
  assert_row "$FIDGET_SCENARIO_AX_DONE" "▸" "done"
  echo "PASS: fixture dumps"
  exit 0
fi

if [ -z "${DISPLAY:-}" ]; then
  echo "SKIP: DISPLAY is unset. X11 thinking-row needs a session, or set FIDGET_SCENARIO_AX_OPEN and FIDGET_SCENARIO_AX_DONE." >&2
  exit 2
fi
python3 -c 'import pyatspi' >/dev/null 2>&1 || {
  echo "SKIP: python3-pyatspi is not installed." >&2
  exit 2
}

test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
out="${TMPDIR:-/tmp}/fidget-scenario-thinking-row-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out/home"
log="$out/app.log" marks="$out/harness.log"
: > "$marks"

harness="$root/scripts/scenarios/fixture-harness.sh $test_bin script=scenario-thinking count=$marks"
[ "$(wc -w <<< "$harness")" -eq 4 ] || fail "a path in the Harness line holds a space: $harness"

env HOME="$out/home" \
  FIDGET_HARNESS="$harness" \
  FIDGET_DIRECTOR_WAKE_SECS=6 \
  FIDGET_DIRECTOR_API_KEY=x FIDGET_CAPTURABLE=1 \
  FIDGET_CHARACTER=bmo FIDGET_CHARACTERS="$root/characters" \
  "$bin" > "$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2> /dev/null || true; pkill -f "count=$marks" || true' EXIT

wait_for() {
  local n=$(($1 * 4))
  shift
  until "$@" > /dev/null 2>&1; do
    kill -0 "$pid" 2> /dev/null || fail "Fidget exited; see $log"
    n=$((n - 1))
    [ "$n" -gt 0 ] || return 1
    sleep 0.25
  done
}

capture() { # <name> <glyph>
  local dump="$out/$1.ax.txt"
  python3 "$root/scripts/ax-window-linux.py" dump "$pid" BMO > "$dump" 2>"$out/$1.err" \
    || fail "$1: AT-SPI dump failed; see $out/$1.err"
  assert_row "$dump" "$2" "$1"
}

wait_for 30 grep -qx asked "$marks" || fail "no wake reached the Harness; see $log"
wait_for 40 grep -qx 'thought 2' "$marks" || fail "no thinking turn; see $log"
capture mid-thought "▾"
wait_for 15 grep -qx replied "$marks" || fail "no reply; see $log"
sleep 1.5
capture after-reply "▸"
echo "PASS: evidence in $out"
