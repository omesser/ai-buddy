#!/usr/bin/env bash
#
# Test the diagnostic function in verify-overlay.sh
#
# Tests that the Keychain-hint message appears when overlays were reported
# but no frames were traced.

set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

# Source the function under test by extracting it from the script
extract_function() {
  sed -n '/^diagnose_no_frames()/,/^}/p' scripts/verify-overlay.sh > "$TEMP_DIR/function.sh"
  # shellcheck disable=SC1091
  . "$TEMP_DIR/function.sh"
}

test_no_frames_with_overlays() {
  echo "Test: diagnose_no_frames with overlays present"

  # Create a log with overlays but no frames
  cat > "$TEMP_DIR/app.log" << 'EOF'
character: Timber Wolf from .../target/debug/characters/timber-wolf
dock: true bounds via CoreDock, 1529x98 at 195,982
overlay: overlay-0 covers 1920x1080 at (0,0)
overlay: overlay-1 covers 1728x1117 at (1920,0)
overlay: 2 display(s); sprite 176x160; Timber Wolf as bmo
updater: Could not fetch a valid release JSON from the remote
EOF

  extract_function

  # Capture output
  output=$(diagnose_no_frames "$TEMP_DIR/app.log" 2>&1)

  # Check for expected messages
  if ! echo "$output" | grep -q "Likely cause.*Keychain"; then
    echo "  FAIL: Expected Keychain hint in diagnostic output"
    echo "  Got: $output"
    return 1
  fi

  if ! echo "$output" | grep -q "AI_BUDDY_DIRECTOR_API_KEY"; then
    echo "  FAIL: Expected AI_BUDDY_DIRECTOR_API_KEY mention in diagnostic output"
    echo "  Got: $output"
    return 1
  fi

  echo "  PASS"
}

test_no_frames_no_overlays() {
  echo "Test: diagnose_no_frames without overlays"

  # Create a log without overlays
  cat > "$TEMP_DIR/app-no-overlay.log" << 'EOF'
some startup message
another line
EOF

  extract_function

  # Capture output
  output=$(diagnose_no_frames "$TEMP_DIR/app-no-overlay.log" 2>&1)

  # Should not mention Keychain when no overlays were reported
  if echo "$output" | grep -q "Keychain"; then
    echo "  FAIL: Should not mention Keychain when no overlays were reported"
    echo "  Got: $output"
    return 1
  fi

  echo "  PASS"
}

echo "Running diagnostic function tests..."
test_no_frames_with_overlays
test_no_frames_no_overlays
echo ""
echo "All diagnostic tests passed."
