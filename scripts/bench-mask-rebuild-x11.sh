#!/usr/bin/env bash
# Benchmark X11 mask rebuild cost for click-through regions (issue #428).
# Measures XShapeCombineMask calls under different scenarios using shipped characters.
#
# Usage: scripts/bench-mask-rebuild-x11.sh [SCENARIO] [DURATION]
#
# Arguments are positional. SCENARIO defaults to 'idle', DURATION to 10 seconds.
#
# Scenarios:
#   idle       - BMO perched, cursor away (expect ~0 rebuilds/sec)
#   walk       - BMO walking under cursor (expect rebuilds at motion rate)
#   fast       - BMO react animation (10 fps) under cursor
#   large      - Black Mage at scale=3 (larger rendered sprite)

set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

scenario="${1:-idle}"
duration="${2:-10}"
bin="target/debug/ai-buddy"

usage() {
  cat << EOF
Usage: $0 [SCENARIO] [DURATION]

Scenarios:
  idle       BMO perched, cursor away (expect ~0 rebuilds/sec)
  walk       BMO walking under cursor (expect rebuilds at motion rate)
  fast       BMO react animation (10 fps) under cursor
  large      Black Mage at scale=3 (larger rendered sprite)

Duration: seconds to measure (default: 10)
EOF
  exit 1
}

case "$scenario" in
  idle | walk | fast | large) ;;
  --help | -h) usage ;;
  *)
    echo "Unknown scenario: $scenario" >&2
    usage
    ;;
esac

[ -x "$bin" ] || {
  echo "no $bin — run: (cd src-tauri && cargo build --bin ai-buddy)" >&2
  exit 2
}

log=$(mktemp -t ai-buddy-mask-rebuild-XXXXXX.log)
trap 'kill -TERM $app_pid 2>/dev/null || true; sleep 1; kill -KILL $app_pid 2>/dev/null || true; rm -f "$log"' EXIT INT TERM

echo "Scenario: $scenario"
echo "Duration: ${duration}s"
echo "Log: $log"
echo ""

# Set environment for tracing mask rebuilds
export AI_BUDDY_TRACE_MASK_REBUILD=1

# Choose character and setup based on scenario
case "$scenario" in
  large)
    export AI_BUDDY_INSTANCES="Black Mage"
    echo "Using Black Mage (scale=3, larger rendered sprite)"
    ;;
  *)
    export AI_BUDDY_INSTANCES="BMO"
    echo "Using BMO (126x128@1x)"
    ;;
esac

# Start the app
"$bin" > "$log" 2>&1 &
app_pid=$!

# Wait for overlays to be created
echo -n "Waiting for overlays..."
for _ in $(seq 30); do
  if grep -q 'overlay: [0-9]* display' "$log" 2> /dev/null; then
    echo " ready"
    break
  fi
  sleep 0.5
done

if ! grep -q 'overlay: [0-9]* display' "$log" 2> /dev/null; then
  echo " FAILED"
  echo "App failed to start or create overlays. Log:"
  cat "$log"
  exit 1
fi

# Let sprite settle into position
sleep 2

echo "Measuring for ${duration}s..."

# For walk and fast scenarios, GUI interaction is needed to trigger animation
# under the cursor. This requires manual setup (perch + cursor positioning)
# or MCP play_behavior calls.
case "$scenario" in
  walk)
    echo "Note: Walking requires cursor over sprite during walk animation"
    echo "      Use perch + MCP play_behavior or manual GUI interaction"
    ;;
  fast)
    echo "Note: Fast animation (BMO react at 10fps) requires cursor over sprite"
    echo "      Trigger via poke or other interaction while measuring"
    ;;
esac

sleep "$duration"

# Kill app and extract metrics from log
kill -TERM "$app_pid" 2> /dev/null || true
wait "$app_pid" 2> /dev/null || true

echo ""
echo "=== Results ==="
echo ""

# Count mask rebuilds and extract timing info
rebuild_count=$(grep -c 'mask_rebuild:' "$log" || echo "0")
echo "Total mask rebuilds: $rebuild_count"

if [ "$rebuild_count" -gt 0 ]; then
  # Extract rebuild times (in ms) and calculate stats
  grep 'mask_rebuild:' "$log" | while IFS= read -r line; do
    # Extract: size, scale, opaque pixels, time
    echo "$line" | sed -n 's/.*mask_rebuild: \([0-9]*\)x\([0-9]*\) @\([0-9]*\)x scale, \([0-9]*\) opaque pixels, \([0-9.]*\) ms/\1 \2 \3 \4 \5/p'
  done > /tmp/mask_rebuild_data.txt

  if [ -s /tmp/mask_rebuild_data.txt ]; then
    echo ""
    echo "Rebuild details:"
    echo "  Width x Height @ Scale | Opaque pixels | Time (ms)"
    echo "  -----------------------------------------------"

    awk '{
      printf "  %dx%d @%dx | %d | %.3f ms\n", $1, $2, $3, $4, $5
      sum += $5
      count++
    } END {
      if (count > 0) {
        printf "\n  Average: %.3f ms/rebuild\n", sum/count
        printf "  Rate: %.1f rebuilds/sec\n", count/'"$duration"'
      }
    }' /tmp/mask_rebuild_data.txt

    rm -f /tmp/mask_rebuild_data.txt
  fi
else
  echo "No mask rebuilds detected during measurement period."
  echo ""
  echo "Expected behavior by scenario:"
  echo "  - idle: 0 rebuilds (cursor away from sprite)"
  echo "  - walk: rebuilds at motion rate when cursor over sprite"
  echo "  - fast: rebuilds during high-fps animation under cursor"
  echo "  - large: cost comparison for scale=3 character"
fi

echo ""
echo "Full log: $log"
echo "(Log will be deleted on exit; save now if needed)"
echo ""

# Keep log file instead of deleting on trap
trap - EXIT INT TERM
