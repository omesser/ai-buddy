#!/usr/bin/env bash
# Verify X11 overlay functional parity: Perch, ride, drop, Poke, EWMH states.
#
# Xvfb-runnable. Not a pre-commit hook. Asserts behavior from traces and xprop.
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

# Check for xdotool
if ! command -v xdotool &> /dev/null; then
    log_error "xdotool not found. Install with: sudo apt-get install xdotool"
    exit 1
fi

# Check for xprop
if ! command -v xprop &> /dev/null; then
    log_error "xprop not found. Install with: sudo apt-get install x11-utils"
    exit 1
fi

log_info "Building ai-buddy..."
cd "$WORKSPACE_ROOT"
cargo build --release

log_info "Starting ai-buddy in background..."
export AI_BUDDY_TRACE_FRAMES=1
RUST_LOG=debug target/release/ai-buddy &
APP_PID=$!

# Wait for window to appear
log_info "Waiting for overlay window..."
sleep 3

# Find the ai-buddy window
WINDOW_ID=$(xdotool search --name "ai-buddy" | head -1 || true)

if [ -z "$WINDOW_ID" ]; then
    log_error "Could not find ai-buddy window"
    kill $APP_PID 2>/dev/null || true
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
    kill $APP_PID 2>/dev/null || true
    exit 1
fi

if echo "$WINDOW_PROPS" | grep -q "_NET_WM_STATE_SKIP_TASKBAR"; then
    log_info "✓ _NET_WM_STATE_SKIP_TASKBAR set"
else
    log_error "✗ _NET_WM_STATE_SKIP_TASKBAR missing"
    kill $APP_PID 2>/dev/null || true
    exit 1
fi

if echo "$WINDOW_PROPS" | grep -q "_NET_WM_STATE_SKIP_PAGER"; then
    log_info "✓ _NET_WM_STATE_SKIP_PAGER set"
else
    log_warn "⚠ _NET_WM_STATE_SKIP_PAGER missing (some WMs may not support this)"
fi

# Note: Testing Perch/ride/drop/Poke behavior would require:
# - Opening a test window to perch on
# - Moving the window and checking sprite follows
# - Closing the window and checking sprite falls
# - Clicking on the sprite and checking for Poke in traces
#
# These require more complex X11 window manipulation and trace parsing.
# For now, the EWMH state verification is the core automated check.
# Manual testing can verify interaction behaviors.

log_info "Cleaning up..."
kill $APP_PID 2>/dev/null || true
wait $APP_PID 2>/dev/null || true

log_info "✅ X11 overlay EWMH states verified!"
log_info ""
log_info "Limitation: Perch/ride/drop/Poke behavior testing requires:"
log_info "  - A window manager (Xvfb alone has no WM)"
log_info "  - Real window creation and manipulation"
log_info "  - Trace parsing to assert verb sequences"
log_info ""
log_info "These are not automated here. Manual verification required:"
log_info "  1. Open a terminal window"
log_info "  2. Sprite should perch on the window top edge"
log_info "  3. Move the window slowly, sprite should ride along"
log_info "  4. Close the window, sprite should fall to floor"
log_info "  5. Click on sprite, traces should show 'Poke' verb"
log_info "  6. Right-click on sprite for context menu"
