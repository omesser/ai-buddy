#!/usr/bin/env bash
# Drive a real CGDisplayReconfiguration against a running ai-buddy and prove the
# overlay that loses its display does not take the process down with it (#868).
#
# Mirroring collapses two logical displays into one, so `available_monitors()`
# drops from 2 to 1 and the frame loop posts `place_overlays` onto the AppKit
# main thread mid-reconfiguration. That is the path where the pre-#954 build
# called `window.close()` on the orphaned overlay and aborted with "Rust cannot
# catch foreign exceptions".
#
# Needs two non-mirrored displays and `displayplacer` (brew install displayplacer).
# The screens flicker for the duration; the layout is restored on every exit
# path, including a failure or an interrupt.
#
# Usage: scripts/verify-display-loss-macos.sh [path/to/ai-buddy]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$WORKSPACE_ROOT"

BINARY="${1:-target/debug/ai-buddy}"
SETTLE="${AI_BUDDY_DISPLAY_SETTLE:-6}"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

log_info() {
  echo -e "${GREEN}[INFO]${NC} $*"
}

fail() {
  echo -e "${RED}[ERROR]${NC} $*"
  exit 1
}

command -v displayplacer > /dev/null || fail "displayplacer not installed"
[ -x "$BINARY" ] || fail "no binary at $BINARY"

LOG="$(mktemp -t ai-buddy-display-loss)"
RESTORE="$(displayplacer list | tail -1)"
case "$RESTORE" in
  displayplacer\ *) ;;
  *) fail "could not read the current layout from displayplacer" ;;
esac

# The restore line is already one fully-specified argument per screen, so the
# mirror set is the first screen's own spec with the second screen's id joined
# onto it. displayplacer rejects a bare `id:a+b` for want of a resolution.
eval "set -- ${RESTORE#displayplacer }"
[ "$#" -eq 2 ] || fail "need exactly two displays, found $#"
ID_MAIN="${1#id:}"
ID_MAIN="${ID_MAIN%% *}"
ID_OTHER="${2#id:}"
ID_OTHER="${ID_OTHER%% *}"
MIRROR="${1/id:$ID_MAIN/id:$ID_MAIN+$ID_OTHER}"

APP_PID=""
# Every exit path lands here, so nothing in it may abort the rest under set -e:
# leaving someone's screens mirrored is worse than any result this reports.
restore() {
  if [ -n "$APP_PID" ]; then
    kill "$APP_PID" 2> /dev/null || true
  fi
  eval "$RESTORE" > /dev/null 2>&1 || echo "restore the layout by hand: $RESTORE"
}
trap restore EXIT INT TERM

log_info "layout on exit: $RESTORE"
log_info "launching $BINARY, log at $LOG"
AI_BUDDY_DIRECTOR_API_KEY="${AI_BUDDY_DIRECTOR_API_KEY:-verify-display-loss}" \
  "$BINARY" > "$LOG" 2>&1 &
APP_PID=$!

for _ in $(seq 40); do
  if grep -q 'overlay: .* display(s)' "$LOG"; then
    break
  fi
  sleep 0.5
done
COVERED="$(sed -n 's/^overlay: \([0-9]*\) display(s).*/\1/p' "$LOG" | head -1)"
[ -n "$COVERED" ] || fail "the app traced no overlay line; is AI_BUDDY_DIRECTOR_API_KEY set?"
[ "$COVERED" -ge 2 ] || fail "the app covered $COVERED display(s); this needs 2"
log_info "covering $COVERED displays"

log_info "mirroring, which takes one overlay's display away"
displayplacer "$MIRROR" > /dev/null
sleep "$SETTLE"

if ! kill -0 "$APP_PID" 2> /dev/null; then
  cat "$LOG"
  fail "the app died on the way in; this is #868"
fi
grep -q 'has no display left to cover' "$LOG" ||
  fail "mirroring did not reach the orphan branch, so this run proves nothing"
log_info "the orphan branch ran and the app survived it"

log_info "unmirroring, which gives the display back"
eval "$RESTORE" > /dev/null
sleep "$SETTLE"

if ! kill -0 "$APP_PID" 2> /dev/null; then
  cat "$LOG"
  fail "the app died on the way out; this is #868"
fi
if grep -q 'Rust cannot catch foreign exceptions' "$LOG"; then
  fail "the abort is in the log even though the process lives"
fi

log_info "PASS: survived both edges of the reconfiguration"
sed -n '/display(s)/,$p' "$LOG"
