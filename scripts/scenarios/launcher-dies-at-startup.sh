#!/usr/bin/env bash
# Scenario: launcher-dies-at-startup (macOS)
# On screen: launches Fidget as BMO with a fixture Harness whose first launch
#   aborts before initialize. The menu bar icon's menu opens and its Chat… row
#   is pressed, so Chat opens and takes focus. Codex is pressed on the landing.
#   Two screenshots of the Chat window. Fidget quits when the scenario ends.
# Input: the menu row and the Codex button are pressed through AXPress. Only
#   if one refuses AXPress does the helper click its centre: two clicks at most.
#   No keys.
# Duration: about 25 s, 2 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: the landing reads "<launcher> failed to start" and names the abort
#   signal; the launcher line is its own code text and no landing text shows a
#   backtick (#1071); the mind line reads "failed to start". Pressing a Harness
#   on the landing launches it again at once, and the mind line then names the
#   live Harness. FIDGET_HARNESS owns the row, so Codex re-picks the fixture.
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
wait_for 20 grep -q 'exited before initialize' "$log" || fail "the launcher never died before initialize; see $log"
[ "$(spawns)" -eq 1 ] || fail "want one spawn before Chat opens, the fixture saw $(spawns)"
echo "ok: the first launch aborted: $(grep -om1 'exited before initialize[^;]*' "$log")"

"$tools/ax" open "$pid" Chat BMO > "$out/open.txt" 2>&1 || fail "the menu's Chat row did not open Chat; see $out/open.txt"
wait_for 15 dump_has failed "|$name · failed to start|" || fail "the mind line never read 'failed to start'; see $out/failed.ax.txt"
failed=$out/failed.ax.txt
shot failed
grep -qF "|$name failed to start|" "$failed" || fail "no '$name failed to start' title in $failed"
grep -qE '^AXStaticText\|[^|]*\|[^|]*exited before initialize, signal: 6 \(SIGABRT\)' "$failed" ||
  fail "the lede does not name the abort signal in $failed"
grep -qF "Then pick $name again, or pick a different Harness below." "$failed" ||
  fail "the lede does not say to pick the Harness again in $failed"
grep -qF "|$harness|" "$failed" ||
  fail "the launcher line is not a text of its own, so it is not drawn as code, in $failed"
! grep -n '`' "$failed" > "$out/backticks.txt" || fail "landing text shows a backtick; see $out/backticks.txt"
echo "ok: the landing names the launcher, the signal and the next step, with no backticks"

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
