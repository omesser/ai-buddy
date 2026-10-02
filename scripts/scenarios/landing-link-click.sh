#!/usr/bin/env bash
# Scenario: landing-link-click (macOS)
# On screen: launches Fidget as BMO with no harness and PATH emptied so npx
#   is missing. Chat opens by itself. The Codex button is pressed, which shows
#   the "needs npx" landing with the nodejs.org install link. The link is
#   clicked via AX, opening it in the default browser. Two screenshots: the
#   landing with the link, and after the click. Fidget quits at the end.
# Input: AXPress on the Codex button and the nodejs.org link; no keys.
# Duration: about 20 s, 90 s at most.
# Grants: Screen Recording and Accessibility for the terminal that runs it.
# Asserts: the landing draws the install link as a clickable element, and
#   clicking it does not navigate the Chat webview (the URL never reaches the
#   webview's document.location). What opens in the browser is visual only.
#
# Usage: landing-link-click.sh --go <fidget binary>
# Without --go it prints this header, which is the takeover prompt, and exits 2.
set -euo pipefail

if [ "${1:-}" != --go ]; then
  sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"
  exit 2
fi
bin=${2:?usage: landing-link-click.sh --go <fidget binary>}
root=$(cd "$(dirname "$0")/../.." && pwd)
out="${TMPDIR:-/tmp}/fidget-scenario-landing-link-click-$(date +%Y%m%d-%H%M%S)"
tools="${TMPDIR:-/tmp}/fidget-scenario-tools"
mkdir -p "$out/home" "$tools"
log="$out/app.log"

fail() {
  printf 'FAIL: %s\nevidence in %s\n' "$*" "$out" >&2
  exit 1
}

[ -x "$tools/window-id" ] || swiftc -O "$root/scripts/scenarios/window-id.swift" -o "$tools/window-id"
[ "$tools/ax" -nt "$root/scripts/ax-settings.swift" ] || swiftc -O "$root/scripts/ax-settings.swift" -o "$tools/ax"

# Empty PATH so npx is not found. Codex will report missing and show the link.
env HOME="$out/home" \
  PATH="" \
  FIDGET_DIRECTOR_WAKE_SECS=600 \
  FIDGET_DIRECTOR_API_KEY=x FIDGET_CAPTURABLE=1 \
  FIDGET_CHARACTER=bmo FIDGET_CHARACTERS="$root/characters" \
  "$bin" > "$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2> /dev/null || true' EXIT

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

has_text() { # <name> <text>...: true when the dump holds any of them
  local f=$out/$1.ax.txt p
  shift
  "$tools/ax" dump "$pid" BMO > "$f" 2>&1 || return 1
  for p; do grep -qF "$p" "$f" && return 0; done
  return 1
}

shot() { screencapture -x -o -l "$("$tools/window-id" "$pid" BMO)" "$out/$1.png"; }

wait_for 30 "$tools/window-id" "$pid" BMO || fail "Chat did not open"
wait_for 15 has_text opened "Connect a Harness to get started" || fail "Chat landing not shown; see $out/opened.ax.txt"
echo "ok: Chat opened with the connect landing"

# Press Codex. With PATH empty, npx is missing, so the landing says so.
"$tools/ax" press-button "$pid" Codex 1 BMO > "$out/press-codex.txt" 2>&1 || fail "could not press Codex; see $out/press-codex.txt"
wait_for 10 has_text needs-npx "Codex needs" "https://nodejs.org/" || fail "no 'needs npx' landing; see $out/needs-npx.ax.txt"
shot needs-npx
echo "ok: the 'Codex needs npx' landing shows the nodejs.org link"

# The link is a span with AXRole AXStaticText, but it lives inside a clickable
# parent (the log's click listener). Find the link text's geometry to click it.
"$tools/ax" dump "$pid" BMO frames > "$out/link.ax.txt" 2>&1 || fail "AX dump with frames failed; see $out/link.ax.txt"
link_line=$(grep -F 'https://nodejs.org/' "$out/link.ax.txt" | head -1) || fail "no link in AX tree; see $out/link.ax.txt"
link_frame=${link_line##*|}
link_x=$(echo "$link_frame" | cut -d, -f1)
link_y=$(echo "$link_frame" | cut -d, -f2)
link_w=$(echo "$link_frame" | cut -d, -f3)
link_h=$(echo "$link_frame" | cut -d, -f4)
click_x=$(echo "$link_x + $link_w / 2" | bc)
click_y=$(echo "$link_y + $link_h / 2" | bc)

# Synthetic click at the link's center. osascript can click at a point.
osascript -e "tell application \"System Events\" to click at {$click_x, $click_y}" > "$out/click.txt" 2>&1 || fail "click failed; see $out/click.txt"
sleep 1
shot after-click

# The webview should still show the same landing, not navigate to nodejs.org.
"$tools/ax" dump "$pid" BMO > "$out/after-click.ax.txt" 2>&1 || fail "post-click AX dump failed; see $out/after-click.ax.txt"
grep -qF "Codex needs" "$out/after-click.ax.txt" || fail "Chat navigated away from the landing; see $out/after-click.ax.txt"
grep -qF "https://nodejs.org/" "$out/after-click.ax.txt" || fail "landing changed after the click; see $out/after-click.ax.txt"
echo "ok: the webview did not navigate; the landing is still shown"

echo "PASS: evidence in $out"
