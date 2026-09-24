#!/usr/bin/env bash
# Better measurement with sustained interaction
set -euo pipefail

WORKSPACE="/workspace"
MEASUREMENTS_DIR="$WORKSPACE/968-measurements"
BIN="$WORKSPACE/target/debug/ai-buddy"

cd "$WORKSPACE"
export DISPLAY=:1
export AI_BUDDY_TRACE_MASK_REBUILD=1
export AI_BUDDY_TRACE_FRAMES=1  # Also trace frame changes

echo "=== Sustained Animation Measurements ==="

# Position for approximate sprite landing location (lower on screen)
CURSOR_X=960
CURSOR_Y=900  # Lower on screen where sprite likely perches

# 1. BMO with continuous pokes to trigger react animations
echo "1. BMO react (10fps) - continuous pokes for 15 seconds..."
export AI_BUDDY_INSTANCES="BMO"

"$BIN" > "$MEASUREMENTS_DIR/bmo-react-sustained.log" 2>&1 &
app_pid=$!

sleep 3
# Wait for overlays
for i in {1..10}; do
  if grep -q 'overlay:' "$MEASUREMENTS_DIR/bmo-react-sustained.log" 2>/dev/null; then
    break
  fi
  sleep 1
done

sleep 3  # Let sprite fully settle and perch

echo "  Starting continuous pokes with cursor at ($CURSOR_X, $CURSOR_Y)..."
xdotool mousemove $CURSOR_X $CURSOR_Y

# Rapid pokes to keep triggering react
for i in {1..30}; do
  xdotool click 1
  sleep 0.5
done

kill -TERM $app_pid 2>/dev/null || true
wait $app_pid 2>/dev/null || true

rebuild_count=$(grep -c 'mask_rebuild:' "$MEASUREMENTS_DIR/bmo-react-sustained.log" || echo "0")
echo "  Result: $rebuild_count total mask rebuilds"
echo ""

# 2. Black Mage with dragging to trigger continuous mask updates
echo "2. Black Mage (scale=3) - with cursor hold/drag..."
export AI_BUDDY_INSTANCES="Black Mage"

"$BIN" > "$MEASUREMENTS_DIR/blackmage-drag.log" 2>&1 &
app_pid=$!

sleep 3
for i in {1..10}; do
  if grep -q 'overlay:' "$MEASUREMENTS_DIR/blackmage-drag.log" 2>/dev/null; then
    break
  fi
  sleep 1
done

sleep 3

echo "  Moving cursor over sprite and holding..."
xdotool mousemove $CURSOR_X $CURSOR_Y
xdotool mousedown 1
sleep 1

# Drag sprite around
echo "  Dragging sprite..."
for x in 1000 1100 1000 900 960; do
  xdotool mousemove $x 850
  sleep 0.5
done

xdotool mouseup 1
sleep 2

kill -TERM $app_pid 2>/dev/null || true
wait $app_pid 2>/dev/null || true

rebuild_count=$(grep -c 'mask_rebuild:' "$MEASUREMENTS_DIR/blackmage-drag.log" || echo "0")
echo "  Result: $rebuild_count total mask rebuilds"
echo ""

# Analyze results
echo "=== Analysis ==="
echo ""
echo "BMO react (10fps fast animation):"
if [ -f "$MEASUREMENTS_DIR/bmo-react-sustained.log" ]; then
  grep 'mask_rebuild:' "$MEASUREMENTS_DIR/bmo-react-sustained.log" | awk '{
    split($2, a, "x")
    split($3, b, "@")
    split($6, c, ",")
    split($7, d, " ")
    width=a[1]
    height=substr(a[2], 1, length(a[2])-1)
    scale=substr(b[2], 1, length(b[2])-2)
    opaque=c[1]
    time=d[1]
    sum+=time
    count++
    if (NR<=3) print "  " $0
  } END {
    if (count > 0) {
      printf "  Average: %.3f ms/rebuild over %d rebuilds\n", sum/count, count
    }
  }'
fi
echo ""

echo "Black Mage (scale=3 large sprite):"
if [ -f "$MEASUREMENTS_DIR/blackmage-drag.log" ]; then
  grep 'mask_rebuild:' "$MEASUREMENTS_DIR/blackmage-drag.log" | awk '{
    split($2, a, "x")
    split($3, b, "@")
    split($6, c, ",")
    split($7, d, " ")
    width=a[1]
    height=substr(a[2], 1, length(a[2])-1)
    scale=substr(b[2], 1, length(b[2])-2)
    opaque=c[1]
    time=d[1]
    sum+=time
    count++
    if (NR<=3) print "  " $0
  } END {
    if (count > 0) {
      printf "  Average: %.3f ms/rebuild over %d rebuilds\n", sum/count, count
    }
  }'
fi

echo ""
echo "=== Measurements complete ==="
