#!/usr/bin/env bash
# Scenario: sign-in-button (macOS)
# On screen: launches Fidget as BMO with a fixture Harness that advertises
#   agent sign-in. The needs-login landing opens Chat. One button press.
#   Three screenshots of the Chat window. Fidget quits when it ends.
# Input: one button press via AXPress (no real click).
# Duration: about 30 s, 2 min at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: the sign-in button shows on the needs-login landing; pressing it
#   shows the waiting line; once authenticate completes, a session opens and
#   the waiting line disappears.
#
# Usage: sign-in-button.sh --go <fidget binary> <fidget test binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
  exit 2
fi
bin=${2:?usage: sign-in-button.sh --go <fidget binary> <fidget test binary>}
test_bin=${3:?usage: sign-in-button.sh --go <fidget binary> <fidget test binary>}
# Absolute: the Harness spawns in the data folder, so a relative path misses.
test_bin=$(cd "$(dirname "$test_bin")" && pwd)/$(basename "$test_bin")
root=$(cd "$(dirname "$0")/../.." && pwd)
out="${TMPDIR:-/tmp}/fidget-scenario-sign-in-button-$(date +%Y%m%d-%H%M%S)"
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
harness="$root/scripts/scenarios/fixture-harness.sh $test_bin script=auth-sign-in-link count=$marks"
[ "$(wc -w <<< "$harness")" -eq 4 ] || fail "a path in the Harness line holds a space: $harness"

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

dump() { # <name>
  "$tools/ax" dump "$pid" BMO > "$out/$1.ax.txt" 2>&1 || fail "$1: AX dump failed; see $out/$1.ax.txt"
}
shot() { # <name>
  local id
  id=$("$tools/window-id" "$pid" BMO) || fail "$1: no Chat window"
  screencapture -x -o -l "$id" "$out/$1.png"
}

wait_for 30 grep -qx spawn "$marks" || fail "Harness never spawned; see $log"
wait_for 15 "$tools/window-id" "$pid" BMO || fail "Chat did not open for needs-login"

dump needs-login
shot needs-login
grep -qE '^AXButton\|Fake login\|' "$out/needs-login.ax.txt" ||
  fail "no Fake login button on needs-login; see $out/needs-login.ax.txt"
grep -qE '^AXStaticText\|.*Run this in a terminal:' "$out/needs-login.ax.txt" ||
  fail "no terminal command hint on needs-login; see $out/needs-login.ax.txt"
echo "ok: needs-login shows Fake login button"

"$tools/ax" press-button "$pid" "Fake login" 1 BMO > "$out/press.txt" 2>&1 ||
  fail "could not press Fake login button; see $out/press.txt"
sleep 1

dump waiting
shot waiting
grep -qE '^AXStaticText\|.*Finish signing in in your browser' "$out/waiting.ax.txt" ||
  fail "no waiting line after button press; see $out/waiting.ax.txt"
grep -qE '^AXStaticText\|.*came from fake-agent' "$out/waiting.ax.txt" ||
  fail "waiting line does not name the Harness; see $out/waiting.ax.txt"
echo "ok: waiting line appears after button press"

wait_for 40 grep -qx 'elicit-url:accept' "$marks" || fail "sign-in form never answered; see $log"
wait_for 15 grep -qx new "$marks" || fail "session did not open after sign-in; see $log"
[ "$(grep -c new "$marks")" -ge 2 ] || fail "session did not open after sign-in; see $log"
sleep 1.5

dump signed-in
shot signed-in
grep -qE '^AXStaticText\|.*fake-agent · session ' "$out/signed-in.ax.txt" ||
  fail "mind line does not name the session; see $out/signed-in.ax.txt"
if grep -qE 'Finish signing in in your browser' "$out/signed-in.ax.txt"; then
  fail "waiting line still present after sign-in; see $out/signed-in.ax.txt"
fi
echo "ok: session opened, waiting line cleared"
echo "PASS: evidence in $out"
