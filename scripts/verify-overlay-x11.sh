#!/usr/bin/env bash
# Verify X11 overlay functional parity: Perch, ride, drop, Poke, EWMH states.
#
# Runs under Xvfb with mesa/EGL support. Fails if any behavior is not observed.
# Run with: scripts/verify-overlay-x11.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

log_info() {
  echo -e "${GREEN}[INFO]${NC} $*"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $*"
}

fail() {
  log_error "$@"
  exit 1
}

[ -n "${DISPLAY:-}" ] || fail "DISPLAY not set. Run under X11 or Xvfb."

for tool in xdotool xprop xwininfo; do
  command -v "$tool" &> /dev/null || fail "$tool not found. Install: sudo apt-get install x11-utils xdotool"
done

# Xvfb has no window manager. EWMH (_NET_CLIENT_LIST, struts) needs one.
WM_STARTED=0
if ! wmctrl -m &> /dev/null && ! xprop -root _NET_SUPPORTING_WM_CHECK &> /dev/null; then
  command -v openbox &> /dev/null || fail "openbox not found. Install: sudo apt-get install openbox"
  log_info "Starting openbox window manager..."
  openbox --replace &
  WM_PID=$!
  WM_STARTED=1
  sleep 2
fi

log_info "Building ai-buddy..."
cd "$WORKSPACE_ROOT"
cargo build --release

log_info "Starting ai-buddy in background with frame tracing..."
export AI_BUDDY_TRACE_FRAMES=1
export RUST_LOG=debug
export LIBGL_ALWAYS_SOFTWARE=1
TRACE_LOG="/tmp/ai-buddy-verify-$$.log"
target/release/ai-buddy > "$TRACE_LOG" 2>&1 &
APP_PID=$!

cleanup() {
  log_info "Cleaning up..."
  kill $APP_PID 2> /dev/null || true
  if [ $WM_STARTED -eq 1 ]; then
    kill "$WM_PID" 2> /dev/null || true
  fi
}
trap cleanup EXIT

log_info "Waiting for overlay window..."
MAX_WAIT=15
WAITED=0
WINDOW_ID=""
while [ $WAITED -lt $MAX_WAIT ]; do
  WINDOW_ID=$(xdotool search --name "ai-buddy" 2> /dev/null | head -1 || true)
  [ -n "$WINDOW_ID" ] && break
  sleep 1
  WAITED=$((WAITED + 1))
done

[ -n "$WINDOW_ID" ] || {
  log_error "Could not find ai-buddy window after ${MAX_WAIT}s"
  log_error "Last 30 lines of trace log:"
  tail -30 "$TRACE_LOG"
  exit 1
}

log_info "Found window ID: $WINDOW_ID"

log_info "Checking EWMH window states..."
WINDOW_PROPS=$(xprop -id "$WINDOW_ID" _NET_WM_STATE)
echo "$WINDOW_PROPS" | grep -q "_NET_WM_STATE_ABOVE" || fail "_NET_WM_STATE_ABOVE missing"
echo "$WINDOW_PROPS" | grep -q "_NET_WM_STATE_SKIP_TASKBAR" || fail "_NET_WM_STATE_SKIP_TASKBAR missing"
log_info "✓ EWMH states verified"

log_info "Waiting for sprite to initialize..."
sleep 3

grep -q "frame:.*\(Falling\|Grounded\)" "$TRACE_LOG" || fail "Sprite did not initialize (no Falling or Grounded state)"
log_info "✓ Sprite initialized"

SPRITE_POS=$(tail -50 "$TRACE_LOG" | grep "frame:" | tail -1 | grep -oP 'sprite\(\K[0-9]+,[0-9]+' || echo "")
[ -n "$SPRITE_POS" ] || fail "Could not determine sprite position from traces"
SPRITE_X=$(echo "$SPRITE_POS" | cut -d, -f1)
SPRITE_Y=$(echo "$SPRITE_POS" | cut -d, -f2)

log_info "Sprite at ($SPRITE_X, $SPRITE_Y)"

# Top edge below the sprite so it falls onto a Perch rather than landing beside it.
WINDOW_Y=$((SPRITE_Y + 50))
log_info "Creating test window at Y=$WINDOW_Y for Perch test..."
xterm -geometry 80x24+$((SPRITE_X - 200))+$WINDOW_Y -title "ai-buddy-test-window" &
TEST_WINDOW_PID=$!
sleep 2

TEST_WINDOW_ID=$(xdotool search --name "ai-buddy-test-window" | head -1 || true)
[ -n "$TEST_WINDOW_ID" ] || fail "Could not create test window"
log_info "Created test window ID: $TEST_WINDOW_ID"

log_info "Waiting for sprite to perch..."
sleep 4

tail -100 "$TRACE_LOG" | grep -q "frame:.*Perched" || fail "Sprite did not perch (no Perched state in traces)"
log_info "✓ Perched state verified"

log_info "Moving test window to test ride behavior..."
for i in {1..10}; do
  xdotool windowmove "$TEST_WINDOW_ID" $((SPRITE_X - 200 + i * 10)) $WINDOW_Y
  sleep 0.3
done

tail -80 "$TRACE_LOG" | grep -qi "hold" || fail "Sprite did not ride (no Hold animation in traces)"
log_info "✓ Ride behavior (Hold) verified"

log_info "Closing test window to test drop..."
kill $TEST_WINDOW_PID 2> /dev/null || true
sleep 2

tail -50 "$TRACE_LOG" | grep -q "frame:.*Falling" || fail "Sprite did not drop (no Falling state after window close)"
log_info "✓ Drop behavior (Falling) verified"

sleep 2
SPRITE_POS=$(tail -20 "$TRACE_LOG" | grep "frame:" | tail -1 | grep -oP 'sprite\(\K[0-9]+,[0-9]+' || echo "")
[ -n "$SPRITE_POS" ] || fail "Could not determine sprite position for Poke test"
SPRITE_X=$(echo "$SPRITE_POS" | cut -d, -f1)
SPRITE_Y=$(echo "$SPRITE_POS" | cut -d, -f2)

log_info "Clicking sprite at ($SPRITE_X, $((SPRITE_Y + 30)))..."
xdotool mousemove "$SPRITE_X" $((SPRITE_Y + 30))
sleep 0.2
xdotool click 1
sleep 1

tail -30 "$TRACE_LOG" | grep -q "verbs:.*Poke" || fail "Click did not produce Poke verb"
log_info "✓ Poke verb verified"

log_info ""
log_info "✅ All X11 overlay behaviors verified:"
log_info "  - EWMH states (_NET_WM_STATE_ABOVE, SKIP_TASKBAR)"
log_info "  - Sprite initialization (Falling/Grounded)"
log_info "  - Perch (sprite on window top edge)"
log_info "  - Ride (Hold animation on window move)"
log_info "  - Drop (Falling state on window close)"
log_info "  - Poke (verb from click)"
log_info ""
log_info "Trace log saved to: $TRACE_LOG"
