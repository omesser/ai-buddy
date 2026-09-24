#!/usr/bin/env bash
# Measure fast animation (BMO react) and large sprite (Black Mage scale=3)
set -euo pipefail

WORKSPACE="/workspace"
MEASUREMENTS_DIR="$WORKSPACE/968-measurements"
BIN="$WORKSPACE/target/debug/ai-buddy"

cd "$WORKSPACE"
export DISPLAY=:1
export AI_BUDDY_TRACE_MASK_REBUILD=1

echo "=== Fast Animation & Large Sprite Measurements ==="
echo "Commit: $(git log -1 --format='%h %s')"
echo ""

# Get display center for cursor positioning
SCREEN_CENTER_X=960  # 1920/2
SCREEN_CENTER_Y=600  # 1200/2

# Function to wait for overlays and position cursor
wait_and_position() {
  local log_file=$1
  local character=$2
  
  for i in {1..15}; do
    if grep -q 'overlay:' "$log_file" 2>/dev/null; then
      echo "  Overlays ready for $character"
      break
    fi
    sleep 1
  done
  
  sleep 2  # Let sprite settle
  
  # Position cursor over sprite (approximate center of screen where sprite should be)
  xdotool mousemove $SCREEN_CENTER_X $SCREEN_CENTER_Y
  echo "  Cursor positioned at ($SCREEN_CENTER_X, $SCREEN_CENTER_Y)"
}

# 1. BMO react (fast animation at 10fps) - trigger with repeated pokes
echo "1. BMO react (fast animation, 10fps) under cursor..."
export AI_BUDDY_INSTANCES="BMO"

"$BIN" > "$MEASUREMENTS_DIR/fast-bmo-react.log" 2>&1 &
app_pid=$!

sleep 2
if ! ps -p $app_pid > /dev/null; then
  echo "ERROR: App failed to start"
  cat "$MEASUREMENTS_DIR/fast-bmo-react.log"
  exit 1
fi

wait_and_position "$MEASUREMENTS_DIR/fast-bmo-react.log" "BMO"

echo "  Triggering react animations with pokes for 10 seconds..."
# Poke repeatedly to trigger react animations (each poke triggers the react animation)
for i in {1..10}; do
  xdotool click 1
  sleep 1  # Wait for react animation to play (it's 2 frames at 10fps = 0.2s, so 1s spacing is plenty)
done

kill -TERM $app_pid 2>/dev/null || true
wait $app_pid 2>/dev/null || true

rebuild_count=$(grep -c 'mask_rebuild:' "$MEASUREMENTS_DIR/fast-bmo-react.log" || echo "0")
echo "  Result: $rebuild_count mask rebuilds during react animations"

if [ "$rebuild_count" -gt 0 ]; then
  echo "  Rebuild details:"
  grep 'mask_rebuild:' "$MEASUREMENTS_DIR/fast-bmo-react.log" | head -10 | sed 's/^/    /'
fi
echo ""

# 2. Black Mage (scale=3) with cursor over sprite - measure idle animations
echo "2. Black Mage (scale=3, large sprite) under cursor - idle animations..."
export AI_BUDDY_INSTANCES="Black Mage"

"$BIN" > "$MEASUREMENTS_DIR/large-blackmage-cursor-over.log" 2>&1 &
app_pid=$!

sleep 2
if ! ps -p $app_pid > /dev/null; then
  echo "ERROR: App failed to start"
  cat "$MEASUREMENTS_DIR/large-blackmage-cursor-over.log"
  exit 1
fi

wait_and_position "$MEASUREMENTS_DIR/large-blackmage-cursor-over.log" "Black Mage"

echo "  Measuring for 10 seconds with cursor over sprite..."
sleep 10

kill -TERM $app_pid 2>/dev/null || true
wait $app_pid 2>/dev/null || true

rebuild_count=$(grep -c 'mask_rebuild:' "$MEASUREMENTS_DIR/large-blackmage-cursor-over.log" || echo "0")
echo "  Result: $rebuild_count mask rebuilds in 10s"

if [ "$rebuild_count" -gt 0 ]; then
  echo "  Rebuild details:"
  grep 'mask_rebuild:' "$MEASUREMENTS_DIR/large-blackmage-cursor-over.log" | head -10 | sed 's/^/    /'
fi
echo ""

echo "=== Measurements complete ==="
echo "Logs saved in $MEASUREMENTS_DIR/"
