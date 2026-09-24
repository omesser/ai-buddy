#!/usr/bin/env bash
# Run all mask rebuild measurements for PR #968
set -euo pipefail

WORKSPACE="/workspace"
MEASUREMENTS_DIR="$WORKSPACE/968-measurements"
BIN="$WORKSPACE/target/debug/ai-buddy"

cd "$WORKSPACE"

echo "=== Mask Rebuild Measurements for PR #968 ==="
echo "Commit: $(git log -1 --format='%h %s')"
echo "Date: $(date -u +%Y-%m-%d)"
echo ""

# Idle perched (BMO, cursor away) - 15 second verification
echo "1. Idle perched (BMO, cursor away)..."
export AI_BUDDY_TRACE_MASK_REBUILD=1
export AI_BUDDY_INSTANCES="BMO"
export DISPLAY=:1

"$BIN" > "$MEASUREMENTS_DIR/idle-bmo-15s.log" 2>&1 &
app_pid=$!

sleep 3
if ! ps -p $app_pid > /dev/null; then
  echo "ERROR: App failed to start for idle test"
  cat "$MEASUREMENTS_DIR/idle-bmo-15s.log"
  exit 1
fi

# Wait for overlays
for i in {1..10}; do
  if grep -q 'overlay:' "$MEASUREMENTS_DIR/idle-bmo-15s.log" 2>/dev/null; then
    echo "  Overlays ready, waiting for settle..."
    break
  fi
  sleep 1
done

sleep 2  # Let sprite settle
echo "  Measuring for 15s..."
sleep 15

kill -TERM $app_pid 2>/dev/null || true
wait $app_pid 2>/dev/null || true

rebuild_count=$(grep -c 'mask_rebuild:' "$MEASUREMENTS_DIR/idle-bmo-15s.log" || echo "0")
echo "  Result: $rebuild_count mask rebuilds in 15s"
echo ""

# Large sprite idle (Black Mage scale=3, cursor away) - 15 seconds
echo "2. Large sprite idle (Black Mage scale=3, cursor away)..."
export AI_BUDDY_INSTANCES="Black Mage"

"$BIN" > "$MEASUREMENTS_DIR/idle-blackmage-15s.log" 2>&1 &
app_pid=$!

sleep 3
if ! ps -p $app_pid > /dev/null; then
  echo "ERROR: App failed to start for Black Mage test"
  cat "$MEASUREMENTS_DIR/idle-blackmage-15s.log"
  exit 1
fi

# Wait for overlays
for i in {1..10}; do
  if grep -q 'overlay:' "$MEASUREMENTS_DIR/idle-blackmage-15s.log" 2>/dev/null; then
    echo "  Overlays ready, waiting for settle..."
    break
  fi
  sleep 1
done

sleep 2
echo "  Measuring for 15s..."
sleep 15

kill -TERM $app_pid 2>/dev/null || true
wait $app_pid 2>/dev/null || true

rebuild_count=$(grep -c 'mask_rebuild:' "$MEASUREMENTS_DIR/idle-blackmage-15s.log" || echo "0")
echo "  Result: $rebuild_count mask rebuilds in 15s"
echo ""

echo "=== Summary written to $MEASUREMENTS_DIR/summary.txt ==="
