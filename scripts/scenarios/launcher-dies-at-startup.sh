#!/usr/bin/env bash
# Scenario: launcher-dies-at-startup (macOS)
# On screen: launches Fidget as BMO with a fixture Harness whose first launch
#   aborts before initialize. The menu bar icon's menu opens and its Chat… row
#   is pressed, so Chat opens and takes focus. Chat is resized to 420 and 320
#   points, then Codex is pressed. Three screenshots. Fidget quits at the end.
# Input: AXPress on the menu row and Codex, a click at the centre only if one
#   refuses it (two at most); resizes through the Accessibility API; no keys.
# Duration: about 30 s, 2 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: a "Harness error" landing titled "Harness couldn't start" names the
#   signal, boxes the captured stderr under "Error output" and the launch line
#   under "Command", says to fix the error first, shows no backtick (#1071). At
#   420 and 320 both boxes end inside Chat, Error output starts above the fold,
#   and nothing scrolls sideways (#1186). Codex, a re-pick under FIDGET_HARNESS, launches it
#   again at once, and the mind line then names the live Harness.
#
# Usage: launcher-dies-at-startup.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
  exit 2
fi
bin=${2:?usage: launcher-dies-at-startup.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: launcher-dies-at-startup.sh --go <fidget binary> <fidget test binary>}
# Absolute: the Harness spawns in the data folder, so a relative path misses.
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
root=$(cd "$(dirname "$0")/../.." && pwd)
out="${TMPDIR:-/tmp}/fidget-scenario-launcher-dies-at-startup-$(date +%Y%m%d-%H%M%S)"
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
# Rebuilt when the source is newer: this scenario needs `open` to take a row.
[ "$tools/ax" -nt "$root/scripts/ax-settings.swift" ] || swiftc -O "$root/scripts/ax-settings.swift" -o "$tools/ax"

# `abort-first` writes a dyld line and aborts on its first spawn, the broken
# Node of #1069, and answers on the next. FIDGET_HARNESS splits on whitespace,
# so no path in it may hold a space.
harness="$test_bin harness::tests::fake_acp_agent --exact --nocapture --test-threads=1 script=abort-first count=$marks"
[ "$(wc -w <<< "$harness")" -eq 7 ] || fail "a path in the Harness line holds a space: $harness"

# No ambient wake inside the run: one would respawn the Harness by itself, and
# the re-pick below would pass without doing anything.
env HOME="$out/home" \
  FIDGET_HARNESS="$harness" \
  FIDGET_DIRECTOR_WAKE_SECS=600 \
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

spawns() { grep -cx spawn "$marks" || true; }
respawned() { [ "$(spawns)" -ge 2 ]; }
# Fixed strings: the launcher is an absolute path, and a path is not a regex.
dump_has() { # <name> <text>...: true when the dump holds any of them
  local f=$out/$1.ax.txt p
  shift
  "$tools/ax" dump "$pid" BMO > "$f" 2>&1 || return 1
  for p; do grep -qF "$p" "$f" && return 0; done
  return 1
}
shot() { screencapture -x -o -l "$("$tools/window-id" "$pid" BMO)" "$out/$1.png"; }

name=${harness%% *}
dyld="dyld[0]: Library not loaded: /opt/homebrew/opt/llhttp/lib/libllhttp.9.3.dylib"
rect() { tr , ' ' <<< "${1##*|}"; } # <dump line>: its frame as "x y w h"

fits() { # <width>: both boxes end inside Chat, and the page never scrolls sideways
  local w=$1 f=$out/$1.ax.txt wx got wr x y ww sw aw row text fold
  "$tools/ax" size "$pid" BMO "$w" 560 > "$out/$w.frame.txt" 2>&1 || fail "$w: resize failed; see $out/$w.frame.txt"
  read -r wx _ got _ < <(tr , ' ' < "$out/$w.frame.txt")
  [ "$got" -eq "$w" ] || fail "$w: Chat is $got points wide after the resize"
  sleep 1
  "$tools/ax" dump "$pid" BMO frames > "$f" 2>&1 || fail "$w: AX dump failed; see $f"
  shot "failed-$w"
  wr=$((wx + got))
  for text in "$dyld" "|$harness|"; do
    row=$(grep -E '^AXStaticText\|' "$f" | grep -m1 -F "$text") || fail "$w: no box holds '$text' in $f"
    read -r x _ ww _ <<< "$(rect "$row")"
    [ $((x + ww)) -le $((wr + 1)) ] || fail "$w: a box ends at $((x + ww)), past the window edge at $wr"
  done
  # The composer covers the log's foot, so the fold is its top, not the window's.
  row=$(grep -E '^AXStaticText\|' "$f" | grep -m1 -F "$dyld")
  read -r _ y _ _ <<< "$(rect "$row")"
  read -r _ fold _ _ <<< "$(rect "$(grep -m1 '^AXTextArea|||Nothing can answer yet|' "$f")")"
  [ "$y" -lt "$fold" ] || fail "$w: Error output starts at y $y, under the composer at $fold"
  read -r _ _ sw _ <<< "$(rect "$(grep -m1 '^AXScrollArea|' "$f")")"
  read -r _ _ aw _ <<< "$(rect "$(grep -m1 '^AXWebArea|' "$f")")"
  [ "$aw" -le $((sw + 1)) ] || fail "$w: the page is $aw points wide in a $sw-point scroll area, so Chat scrolls sideways"
  echo "ok: $w: both boxes inside the window, Error output at y $y above the fold at $fold, page $aw of $sw points"
}
wait_for 20 grep -q 'exited before initialize' "$log" || fail "the launcher never died before initialize; see $log"
[ "$(spawns)" -eq 1 ] || fail "want one spawn before Chat opens, the fixture saw $(spawns)"
echo "ok: the first launch aborted: $(grep -om1 'exited before initialize[^;]*' "$log")"

"$tools/ax" open "$pid" Chat BMO > "$out/open.txt" 2>&1 || fail "the menu's Chat row did not open Chat; see $out/open.txt"
wait_for 15 dump_has failed "|$name · failed to start|" || fail "the mind line never read 'failed to start'; see $out/failed.ax.txt"
failed=$out/failed.ax.txt
grep -qiF "|harness error|" "$failed" || fail "no 'Harness error' kicker in $failed"
grep -qF "|Harness couldn't start|" "$failed" || fail "no 'Harness couldn't start' title in $failed"
grep -qF "|It exited before initialize, signal: 6 (SIGABRT). This is what it printed:|" "$failed" ||
  fail "the lede does not name the abort signal in $failed"
grep -qE '^AXHeading\|Error output\|' "$failed" || fail "no 'Error output' label in $failed"
grep -qE '^AXHeading\|Command\|' "$failed" || fail "no 'Command' label in $failed"
grep -E '^AXStaticText\|' "$failed" | grep -qF "$dyld" || fail "the fixture's stderr is not in the Error output box in $failed"
grep -qF "|$harness|" "$failed" || fail "the launch line is not boxed whole in $failed"
grep -qF "|Fix the error above, then pick it again, or pick a different Harness below.|" "$failed" ||
  fail "the next step does not say to fix the error first in $failed"
! grep -n '`' "$failed" > "$out/backticks.txt" || fail "landing text shows a backtick; see $out/backticks.txt"
echo "ok: the landing boxes the captured stderr and the launch line, and says to fix the error first"
for w in 420 320; do fits "$w"; done

[ "$(spawns)" -eq 1 ] || fail "the Harness launched again before the re-pick ($(spawns) spawns)"
"$tools/ax" press-button "$pid" Codex 1 BMO > "$out/press.txt" 2>&1 || fail "could not press Codex on the landing; see $out/press.txt"
wait_for 10 respawned ||
  fail "the re-pick did not launch the Harness again; see $log"
wait_for 15 dump_has live "|$name · session " "|$name · no session yet|" ||
  fail "after the re-pick the mind line names no live Harness; see $out/live.ax.txt"
shot live
! grep -qF 'failed to start' "$out/live.ax.txt" || fail "'failed to start' still shows after the re-pick; see $out/live.ax.txt"
echo "ok: the re-pick launched it again at once: $(grep -oEm1 '· (session [^|]+|no session yet)' "$out/live.ax.txt")"
echo "PASS: evidence in $out"
