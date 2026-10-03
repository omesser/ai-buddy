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
# Asserts: the bubble shows "Question for you in the chat" after the Poke
#   lands while the question waits.
#
# Usage: question-bubble.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
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

[ -x "$tools/window-id" ] || swiftc -O "$root/scripts/scenarios/window-id.swift" -o "$tools/window-id"
[ -x "$tools/click-cursor" ] || swiftc -O "$root/scripts/click-cursor.swift" -o "$tools/click-cursor"
[ -x "$tools/ax" ] || swiftc -O "$root/scripts/ax-settings.swift" -o "$tools/ax"

# FIDGET_HARNESS splits on whitespace, so no path in it may hold a space.
harness="$test_bin harness::tests::fake_acp_agent --exact --nocapture --test-threads=1 script=scenario-asking count=$marks"
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

# Wait for the question to be asked and Chat to open.
wait_for 30 grep -qx asked "$marks" || fail "no wake reached the Harness; see $log"
sleep 1

# Overlay window is titled "Fidget" (build_overlay in src-tauri/src/main.rs).
# Get window id and bounds for the Poke click.
read -r id x y w h < <("$tools/window-id" -b "$pid" Fidget) || fail "no Fidget overlay window"

# Capture the overlay before the Poke.
screencapture -x -o -l "$id" "$out/before-poke.png"

# Send a Poke: one click on the sprite's center.
cx=$((x + w / 2))
cy=$((y + h / 2))
"$tools/click-cursor" "$cx" "$cy" 1 > "$out/poke.txt" 2>&1 || fail "could not click the sprite; see $out/poke.txt"
sleep 1

# Capture the overlay after the Poke, showing the bubble.
screencapture -x -o -l "$id" "$out/after-poke.png"

# Check the bubble content via the Accessibility API.
"$tools/ax" dump "$pid" Fidget > "$out/after-poke.ax.txt" 2>&1 || fail "AX dump failed; see $out/after-poke.ax.txt"
if grep -qF "Question for you in the chat" "$out/after-poke.ax.txt"; then
  echo "ok: bubble shows 'Question for you in the chat'"
elif grep -qF "Question for you in the " "$out/after-poke.ax.txt"; then
  echo "ok: bubble shows 'Question for you in the ' with link"
else
  fail "bubble does not show the question cue; see $out/after-poke.ax.txt"
fi

echo "PASS: evidence in $out"
