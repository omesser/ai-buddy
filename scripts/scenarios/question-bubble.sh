#!/usr/bin/env bash
# Scenario: question-bubble (macOS)
# On screen: launches Fidget as BMO with a fixture Harness. Chat opens by
#   itself and shows a permission question. A Poke is sent to the sprite while
#   the question waits. Two screenshots of the overlay: one before the Poke,
#   one after showing the "Question for you in the chat" bubble. Fidget quits
#   when the scenario ends.
# Input: one real click on the sprite (the Poke).
# Duration: about 20 s, 1 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: no "Question for you" bubble before the Poke, and after it lands
#   while the question waits, the bubble's text and its "chat" link button.
#
# Usage: question-bubble.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0" || true
  exit 2
fi
bin=${2:?usage: question-bubble.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: question-bubble.sh --go <fidget binary> <fidget test binary>}
# Absolute: the Harness spawns in the data folder, so a relative path misses.
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
root=$(cd "$(dirname "$0")/../.." && pwd)
out="${TMPDIR:-/tmp}/fidget-scenario-question-bubble-$(date +%Y%m%d-%H%M%S)"
tools="${TMPDIR:-/tmp}/fidget-scenario-tools"
mkdir -p "$out/home" "$tools"
log="$out/app.log" marks="$out/harness.log"
: > "$marks"

fail() {
  echo "FAIL: $*" >&2
  echo "evidence in $out" >&2
  exit 1
}

[ -x "$tools/click-cursor" ] || swiftc -O "$root/scripts/click-cursor.swift" -o "$tools/click-cursor"
[ "$tools/ax" -nt "$root/scripts/ax-settings.swift" ] || swiftc -O "$root/scripts/ax-settings.swift" -o "$tools/ax"

# FIDGET_HARNESS splits on whitespace, so no path in it may hold a space.
harness="$root/scripts/scenarios/fixture-harness.sh $test_bin script=scenario-asking count=$marks"
[ "$(wc -w <<< "$harness")" -eq 4 ] || fail "a path in the Harness line holds a space: $harness"

env HOME="$out/home" \
  FIDGET_HARNESS="$harness" \
  FIDGET_DIRECTOR_WAKE_SECS=6 \
  FIDGET_DIRECTOR_API_KEY=x FIDGET_CAPTURABLE=1 FIDGET_TRACE_FRAMES=1 \
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

# Wait for the question to be asked and Chat to open.
wait_for 30 grep -qx asked "$marks" || fail "no wake reached the Harness; see $log"
sleep 1

# The overlay spans the display, so its centre is not the sprite. The newest
# frame trace line says where the sprite is drawn, in global points.
read -r sw sh < <(sed -nE 's/.*sprite ([0-9]+)x([0-9]+);.*/\1 \2/p' "$log" | head -1) || fail "no sprite size in $log"
read -r sx sy < <(sed -nE 's/^frame: .* sprite\((-?[0-9]+),(-?[0-9]+)\) .*/\1 \2/p' "$log" | tail -1) || fail "no frame trace in $log"
rect="$((sx - sw)),$((sy - 2 * sh)),$((3 * sw)),$((3 * sh))"
screencapture -x -R "$rect" "$out/before-poke.png"
"$tools/ax" dump "$pid" Fidget > "$out/before-poke.ax.txt" 2>&1 || fail "AX dump failed; see $out/before-poke.ax.txt"
! grep -qF "Question for you" "$out/before-poke.ax.txt" || fail "the question cue showed before the Poke; see $out/before-poke.ax.txt"

# Send a Poke: one click on the sprite's center.
"$tools/click-cursor" "$((sx + sw / 2))" "$((sy + sh / 2))" 1 > "$out/poke.txt" 2>&1 || fail "could not click the sprite; see $out/poke.txt"
wait_for 3 grep -q '^verbs: .*Poke' "$log" || fail "the click did not land as a Poke; see $log"
sleep 1
screencapture -x -R "$rect" "$out/after-poke.png"

# Check the bubble content via the Accessibility API.
"$tools/ax" dump "$pid" Fidget > "$out/after-poke.ax.txt" 2>&1 || fail "AX dump failed; see $out/after-poke.ax.txt"
if ! grep -qF "AXStaticText||Question for you in the |" "$out/after-poke.ax.txt" ||
  ! grep -qF "AXButton|chat|" "$out/after-poke.ax.txt"; then
  fail "bubble does not show the question cue and its chat link; see $out/after-poke.ax.txt"
fi
echo "ok: bubble shows 'Question for you in the' and a 'chat' link button"

echo "PASS: evidence in $out"
