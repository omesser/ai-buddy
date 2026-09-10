#!/usr/bin/env bash
# Runnable-subset proof: core tests, renderer tests, overlay diagnostic script.
# Writes evidence that survives cleanup. Use when GUI/overlay cannot run.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
# shellcheck disable=SC1091
. "$SCRIPT_DIR/common.sh"

cd "$REPO_ROOT"
DEST="$AI_BUDDY_VERIFY_EVIDENCE/units"
mkdir -p "$DEST"

STATUS=0

echo "prove-units: cargo test -p ai-buddy-core"
if cargo test -p ai-buddy-core 2>&1 | tee "$DEST/cargo-core.txt"; then
  echo "PASS cargo-core" | tee -a "$DEST/summary.txt"
else
  echo "FAIL cargo-core" | tee -a "$DEST/summary.txt"
  STATUS=1
fi

echo "prove-units: node --test"
if node --test tests/*.test.js 2>&1 | tee "$DEST/node-tests.txt"; then
  echo "PASS node-tests" | tee -a "$DEST/summary.txt"
else
  echo "FAIL node-tests" | tee -a "$DEST/summary.txt"
  STATUS=1
fi

echo "prove-units: test_verify_overlay_diagnostics.sh"
if bash scripts/test_verify_overlay_diagnostics.sh 2>&1 | tee "$DEST/overlay-diagnostics.txt"; then
  echo "PASS overlay-diagnostics" | tee -a "$DEST/summary.txt"
else
  echo "FAIL overlay-diagnostics" | tee -a "$DEST/summary.txt"
  STATUS=1
fi

# Document GUI gap for this host so Oded is not left a chore list.
{
  echo "# GUI / overlay gap (this host)"
  echo
  echo "- OS: $(uname -s) $(uname -m)"
  echo "- DISPLAY=${DISPLAY:-<unset>}"
  echo "- xterm: $(command -v xterm || echo MISSING)"
  echo "- openbox: $(command -v openbox || echo MISSING)"
  echo "- xdotool: $(command -v xdotool || echo MISSING)"
  if [ -z "${DISPLAY:-}" ] || ! command -v xterm > /dev/null || ! command -v openbox > /dev/null; then
    echo
    echo "\`scripts/verify-overlay-x11.sh\` was not runnable here without \`DISPLAY\` + \`xterm\` + supporting WM/\`openbox\`."
    echo "Overlay presence / poke / summon GUI paths remain covered by that script on a proper X11 desktop;"
    echo "this evidence pack proves the unit + diagnostic subset only."
  fi
} > "$DEST/GUI-GAP.md"

if [ "$STATUS" -eq 0 ]; then
  append_proof "prove-units PASS — see $DEST (GUI gap noted in GUI-GAP.md if any)"
else
  append_proof "prove-units FAIL — see $DEST"
fi
exit "$STATUS"
