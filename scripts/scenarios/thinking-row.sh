#!/usr/bin/env bash
# Scenario: thinking-row (macOS)
# On screen: launches Fidget as BMO with a fixture Harness. Chat opens by
#   itself and takes focus. Two screenshots of the Chat window. Fidget quits
#   when the scenario ends.
# Input: none.
# Duration: about 30 s, 2 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: the Thinking row is open while the Harness thinks, and collapsed
#   once the reply lands.
#
# Usage: thinking-row.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
  exit 2
fi
bin=${2:?usage: thinking-row.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: thinking-row.sh --go <fidget binary> <fidget test binary>}
# Absolute: the Harness spawns in the data folder, so a relative path misses.
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
root=$(cd "$(dirname "$0")/../.." && pwd)
out="${TMPDIR:-/tmp}/fidget-scenario-thinking-row-$(date +%Y%m%d-%H%M%S)"
tools="${TMPDIR:-/tmp}/fidget-scenario-tools"
mkdir -p "$out/home" "$tools"
log="$out/app.log" marks="$out/harness.log"
: > "$marks"

fail() {
  echo "FAIL: $*" >&2
  echo "evidence in $out" >&2
  exit 1
}

[ -x "$tools/window-id" ] || swiftc -O "$root/scripts/scenarios/window-id.swift" -o "$tools/window-id"
[ -x "$tools/ax" ] || swiftc -O "$root/scripts/ax-settings.swift" -o "$tools/ax"

# FIDGET_HARNESS splits on whitespace, so no path in it may hold a space.
harness="$test_bin harness::tests::fake_acp_agent --exact --nocapture --test-threads=1 script=scenario-thinking count=$marks"
[ "$(wc -w <<< "$harness")" -eq 7 ] || fail "a path in the Harness line holds a space: $harness"

env HOME="$out/home" \
  FIDGET_HARNESS="$harness" \
  FIDGET_DIRECTOR_WAKE_SECS=6 \
  FIDGET_DIRECTOR_API_KEY=x FIDGET_CAPTURABLE=1 \
  FIDGET_CHARACTER=bmo FIDGET_CHARACTERS="$root/characters" \
  "$bin" > "$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2> /dev/null || true; pkill -f "count=$marks" || true' EXIT

wait_for() { # <seconds> <command...>
  local n=$(($1 * 4))
  shift
  until "$@" > /dev/null 2>&1; do
    kill -0 "$pid" 2> /dev/null || fail "Fidget exited; see $log"
    n=$((n - 1))
    [ "$n" -gt 0 ] || return 1
    sleep 0.25
  done
}

# The first row is the turn that thought; the ask turn before it has none.
capture() { # <name> <glyph the first Thinking row starts with>
  local id row
  id=$("$tools/window-id" "$pid" BMO) || fail "$1: no Chat window"
  screencapture -x -o -l "$id" "$out/$1.png"
  "$tools/ax" dump "$pid" BMO > "$out/$1.ax.txt" 2>&1 || fail "$1: AX dump failed; see $out/$1.ax.txt"
  row=$(grep -iEm1 '^AXButton\|(▸|▾) thinking' "$out/$1.ax.txt") || fail "$1: no Thinking row in $out/$1.ax.txt"
  [[ $row == "AXButton|$2"* ]] || fail "$1: Thinking row reads '$row', want it to start with $2"
  echo "ok: $1: $row"
}

wait_for 30 grep -qx asked "$marks" || fail "no wake reached the Harness; see $log"
wait_for 15 "$tools/window-id" "$pid" BMO || fail "Chat did not open for the ask"
wait_for 40 grep -qx 'thought 2' "$marks" || fail "no thinking turn; see $log"
capture mid-thought ▾
wait_for 15 grep -qx replied "$marks" || fail "no reply; see $log"
sleep 1.5
capture after-reply ▸
echo "PASS: evidence in $out"
