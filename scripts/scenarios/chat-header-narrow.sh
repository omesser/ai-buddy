#!/usr/bin/env bash
# Scenario: chat-header-narrow (macOS)
# On screen: launches Fidget as BMO with a fixture Harness. Chat opens by
#   itself and takes focus, then is resized to 420, 360 and 320 points wide.
#   One screenshot of the Chat window per width. Fidget quits when it ends.
# Input: none. The resizes go through the Accessibility API, not the mouse.
# Duration: about 30 s, 2 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: at each width the Instance name, the Character chip and the mind
#   line share one row, all three end inside the window, and the page is no
#   wider than its scroll area, so Chat never scrolls sideways. The mind line
#   names the fixture's long unbroken path, the shape #1090 reports.
#
# Usage: chat-header-narrow.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
  exit 2
fi
bin=${2:?usage: chat-header-narrow.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: chat-header-narrow.sh --go <fidget binary> <fidget test binary>}
# Absolute: the Harness spawns in the data folder, so a relative path misses.
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
root=$(cd "$(dirname "$0")/../.." && pwd)
out="${TMPDIR:-/tmp}/fidget-scenario-chat-header-narrow-$(date +%Y%m%d-%H%M%S)"
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
# Rebuilt when the source is newer: this scenario needs its `size` and `frames`.
[ "$tools/ax" -nt "$root/scripts/ax-settings.swift" ] || swiftc -O "$root/scripts/ax-settings.swift" -o "$tools/ax"

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

mind='^AXStaticText\|[^|]*\|[^|]* · session [^|]+\|'
dump() { # <name>
  "$tools/ax" dump "$pid" BMO frames > "$out/$1.ax.txt" 2>&1
}
shows_session() { dump session && grep -qE "$mind" "$out/session.ax.txt"; }
rect() { # <dump line>: its frame as "x y w h"
  tr , ' ' <<< "${1##*|}"
}

check() { # <width>
  local w=$1 f=$out/$1.ax.txt got x y ww h wx wr sw aw rows label top=-1 bottom=-1
  "$tools/ax" size "$pid" BMO "$w" 560 > "$out/$w.frame.txt" 2>&1 || fail "$w: resize failed; see $out/$w.frame.txt"
  read -r wx _ got _ < <(tr , ' ' < "$out/$w.frame.txt")
  [ "$got" -eq "$w" ] || fail "$w: Chat is $got points wide after the resize"
  sleep 1
  dump "$w" || fail "$w: AX dump failed; see $f"
  screencapture -x -o -l "$("$tools/window-id" "$pid" BMO)" "$out/$w.png"
  wr=$((wx + got))

  # The two texts before the mind line are the Instance name and the chip.
  rows=$(grep -E '^AXStaticText\|' "$f" | grep -EB2 -m1 "$mind") ||
    fail "$w: no mind line naming a session in $f"
  [ "$(wc -l <<< "$rows")" -eq 3 ] || fail "$w: no name and chip before the mind line in $f"
  while IFS= read -r row; do
    read -r x y ww h <<< "$(rect "$row")"
    label=$(cut -d'|' -f3 <<< "$row")
    [ $((x + ww)) -le $((wr + 1)) ] || fail "$w: '$label' ends at $((x + ww)), past the window edge at $wr"
    if [ "$top" -lt 0 ]; then
      top=$y bottom=$((y + h))
    else
      [ "$y" -lt "$bottom" ] && [ $((y + h)) -gt "$top" ] || fail "$w: '$label' at y $y..$((y + h)) left the name's row $top..$bottom"
    fi
  done <<< "$rows"

  read -r _ _ sw _ <<< "$(rect "$(grep -m1 '^AXScrollArea|' "$f")")"
  read -r _ _ aw _ <<< "$(rect "$(grep -m1 '^AXWebArea|' "$f")")"
  [ "$aw" -le $((sw + 1)) ] || fail "$w: the page is $aw points wide in a $sw-point scroll area, so Chat scrolls sideways"
  echo "ok: $w: one row inside the window, page $aw of $sw points"
}

wait_for 30 grep -qx asked "$marks" || fail "no wake reached the Harness; see $log"
wait_for 15 "$tools/window-id" "$pid" BMO || fail "Chat did not open for the ask"
wait_for 30 shows_session || fail "the mind line names no session; see $out/session.ax.txt"
echo "ok: $(grep -oEm1 '[^|]* · session [^|]+' "$out/session.ax.txt")"
for w in 420 360 320; do check "$w"; done
echo "PASS: evidence in $out"
