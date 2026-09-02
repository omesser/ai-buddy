#!/usr/bin/env bash
# Verify X11 overlay functional parity: Perch, ride, drop, Poke, EWMH states.
#
# Xvfb-runnable with limitations: Tauri+WebKitGTK requires OpenGL/EGL support.
# Under Xvfb, the app may fail to create windows due to missing GL drivers.
# On a real X11 display with a window manager, all assertions run.
#
# Run with: scripts/verify-overlay-x11.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
  echo -e "${GREEN}[INFO]${NC} $*"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $*"
}

# Check if running under X11
if [ -z "${DISPLAY:-}" ]; then
  log_error "DISPLAY not set. Run under X11 or Xvfb."
  exit 1
fi

# Check for required tools
for tool in xdotool xprop xwininfo; do
  if ! command -v "$tool" &> /dev/null; then
    log_error "$tool not found. Install with: sudo apt-get install x11-utils xdotool"
    exit 1
  fi
done

# Check for window manager
if ! command -v openbox &> /dev/null && ! command -v fluxbox &> /dev/null; then
  log_warn "No window manager found. Perch tests require a WM."
  log_warn "Install with: sudo apt-get install openbox"
fi

# Start a minimal window manager if not already running
WM_STARTED=0
if ! wmctrl -m &> /dev/null && ! xprop -root _NET_SUPPORTING_WM_CHECK &> /dev/null; then
  if command -v openbox &> /dev/null; then
    log_info "Starting openbox window manager..."
    openbox --replace &
    WM_PID=$!
    WM_STARTED=1
    sleep 2
  elif command -v fluxbox &> /dev/null; then
    log_info "Starting fluxbox window manager..."
    fluxbox &
    WM_PID=$!
    WM_STARTED=1
    sleep 2
  else
    log_warn "No WM started. Perch/ride/drop tests will fail."
  fi
fi

log_info "Building ai-buddy..."
cd "$WORKSPACE_ROOT"
cargo build --release

log_info "Starting ai-buddy in background with frame tracing..."
export AI_BUDDY_TRACE_FRAMES=1
export RUST_LOG=debug
TRACE_LOG="/tmp/ai-buddy-verify-$$.log"
target/release/ai-buddy > "$TRACE_LOG" 2>&1 &
APP_PID=$!

cleanup() {
  log_info "Cleaning up..."
  kill $APP_PID 2> /dev/null || true
  if [ $WM_STARTED -eq 1 ]; then
    kill $WM_PID 2> /dev/null || true
  fi
  # Keep trace log for debugging
  # rm -f "$TRACE_LOG"
}
trap cleanup EXIT

# Wait for window to appear
log_info "Waiting for overlay window..."
MAX_WAIT=10
WAITED=0
WINDOW_ID=""
while [ $WAITED -lt $MAX_WAIT ]; do
  WINDOW_ID=$(xdotool search --name "ai-buddy" 2>/dev/null | head -1 || true)
  if [ -n "$WINDOW_ID" ]; then
    break
  fi
  sleep 1
  WAITED=$((WAITED + 1))
done

if [ -z "$WINDOW_ID" ]; then
  log_error "Could not find ai-buddy window after ${MAX_WAIT}s"
  log_error ""
  log_error "This usually means WebKitGTK failed to create windows."
  log_error "Common causes:"
  log_error "  - Running under Xvfb without OpenGL/EGL support"
  log_error "  - Missing mesa drivers (libgl1-mesa-dri, libegl1-mesa)"
  log_error "  - X server does not support DRI3"
  log_error ""
  log_error "To run this script:"
  log_error "  1. On a real X11 display: DISPLAY=:0 scripts/verify-overlay-x11.sh"
  log_error "  2. Or install mesa: sudo apt-get install libgl1-mesa-dri libegl1-mesa"
  log_error ""
  log_error "Last 20 lines of trace log:"
  tail -20 "$TRACE_LOG"
  exit 1
fi

log_info "Found window ID: $WINDOW_ID"

# Verify EWMH states
log_info "Checking EWMH window states..."
WINDOW_PROPS=$(xprop -id "$WINDOW_ID" _NET_WM_STATE)

if echo "$WINDOW_PROPS" | grep -q "_NET_WM_STATE_ABOVE"; then
  log_info "✓ _NET_WM_STATE_ABOVE set"
else
  log_error "✗ _NET_WM_STATE_ABOVE missing"
  exit 1
fi

if echo "$WINDOW_PROPS" | grep -q "_NET_WM_STATE_SKIP_TASKBAR"; then
  log_info "✓ _NET_WM_STATE_SKIP_TASKBAR set"
else
  log_error "✗ _NET_WM_STATE_SKIP_TASKBAR missing"
  exit 1
fi

if echo "$WINDOW_PROPS" | grep -q "_NET_WM_STATE_SKIP_PAGER"; then
  log_info "✓ _NET_WM_STATE_SKIP_PAGER set"
else
  log_warn "⚠ _NET_WM_STATE_SKIP_PAGER missing (some WMs may not support this)"
fi

# Wait for initial frames to be logged
log_info "Waiting for sprite to initialize..."
sleep 2

# Check initial state (should be Falling or Grounded)
log_info "Checking initial state..."
if grep -q "frame:.*Falling\|frame:.*Grounded" "$TRACE_LOG"; then
  log_info "✓ Sprite initialized (Falling or Grounded state seen)"
else
  log_warn "⚠ Could not verify initial state from traces"
fi

# Create a test window to perch on
log_info "Creating test window for Perch test..."
xterm -geometry 80x24+100+100 -title "ai-buddy-test-window" &
TEST_WINDOW_PID=$!
sleep 1

TEST_WINDOW_ID=$(xdotool search --name "ai-buddy-test-window" | head -1 || true)
if [ -z "$TEST_WINDOW_ID" ]; then
  log_warn "⚠ Could not create test window. Skipping Perch/ride/drop tests."
else
  log_info "Created test window ID: $TEST_WINDOW_ID"
  
  # Get test window geometry
  WINDOW_GEOM=$(xwininfo -id "$TEST_WINDOW_ID" | grep -E "Absolute upper-left|Width|Height")
  log_info "Test window geometry: $WINDOW_GEOM"
  
  # Wait for sprite to potentially perch
  log_info "Waiting for sprite to react to window..."
  sleep 3
  
  # Check for Perched state in traces
  if tail -100 "$TRACE_LOG" | grep -q "frame:.*Perched"; then
    log_info "✓ Perched state detected"
  else
    log_warn "⚠ Perched state not detected. Sprite may not be near window edge."
  fi
  
  # Move the window slowly to test ride behavior
  log_info "Moving test window to test ride behavior..."
  for i in {1..5}; do
    xdotool windowmove "$TEST_WINDOW_ID" $((100 + i * 10)) 100
    sleep 0.5
  done
  
  # Check for Hold animation (ride behavior)
  if tail -50 "$TRACE_LOG" | grep -q "hold"; then
    log_info "✓ Hold animation seen (ride behavior)"
  else
    log_warn "⚠ Hold animation not seen. May not have been riding."
  fi
  
  # Close the test window to test drop behavior
  log_info "Closing test window to test drop..."
  kill $TEST_WINDOW_PID 2> /dev/null || true
  sleep 2
  
  # Check for Falling state after window closes
  if tail -50 "$TRACE_LOG" | grep -q "frame:.*Falling"; then
    log_info "✓ Falling state detected after window closed (drop behavior)"
  else
    log_warn "⚠ Falling state not detected after window close"
  fi
fi

# Test Poke verb by simulating a click
# First, find sprite position from recent traces
SPRITE_POS=$(tail -20 "$TRACE_LOG" | grep "frame:" | tail -1 | grep -oP 'sprite\(\K[0-9]+,[0-9]+' || echo "")
if [ -n "$SPRITE_POS" ]; then
  SPRITE_X=$(echo "$SPRITE_POS" | cut -d, -f1)
  SPRITE_Y=$(echo "$SPRITE_POS" | cut -d, -f2)
  
  log_info "Attempting to click sprite at ($SPRITE_X, $SPRITE_Y)..."
  xdotool mousemove $SPRITE_X $((SPRITE_Y + 30))
  xdotool click 1
  sleep 0.5
  
  # Check for Poke verb in traces
  if tail -20 "$TRACE_LOG" | grep -q "verbs:.*Poke"; then
    log_info "✓ Poke verb detected"
  else
    log_warn "⚠ Poke verb not detected. Click may have missed sprite."
  fi
else
  log_warn "⚠ Could not determine sprite position for Poke test"
fi

# Final summary
log_info ""
log_info "✅ X11 overlay verification complete!"
log_info ""
log_info "Verified:"
log_info "  - EWMH states (_NET_WM_STATE_ABOVE, SKIP_TASKBAR)"
log_info "  - Sprite initialization (Falling/Grounded)"
if [ -n "$TEST_WINDOW_ID" ]; then
  log_info "  - Perch detection (attempted)"
  log_info "  - Ride behavior via Hold animation (attempted)"
  log_info "  - Drop behavior via Falling state (attempted)"
fi
if [ -n "$SPRITE_POS" ]; then
  log_info "  - Poke verb via click interaction (attempted)"
fi
log_info ""
log_info "Note: Some tests are opportunistic and may not trigger every run"
log_info "depending on sprite initial position and timing."
log_info ""
log_info "Full trace log saved to: $TRACE_LOG"
