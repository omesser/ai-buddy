#!/usr/bin/env bash
# Read-only: is this checkout worth driving?
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
# shellcheck disable=SC1091
. "$SCRIPT_DIR/common.sh"

UNITS=0
[ "${1:-}" = "--units" ] && UNITS=1

cd "$REPO_ROOT"
fails=0
pass() { echo "  PASS  $*"; }
fail() {
  echo "  FAIL  $*"
  fails=$((fails + 1))
}

echo "doctor: repo=$REPO_ROOT RUN_ID=$RUN_ID"
echo "doctor: evidence=$AI_BUDDY_VERIFY_EVIDENCE"

if [ -f Cargo.toml ] && [ -d src-tauri ]; then
  pass "workspace layout (Cargo.toml + src-tauri)"
else
  fail "not an ai-buddy checkout"
fi

if BIN=$(ai_buddy_bin); then
  pass "binary present: $BIN"
else
  if command -v cargo > /dev/null; then
    pass "no binary yet; cargo is available to build"
  else
    fail "no ai-buddy binary and no cargo"
  fi
fi

OS="$(uname -s)"
case "$OS" in
  Darwin)
    if command -v swift > /dev/null; then
      pass "swift on PATH"
    else
      fail "swift missing (macOS verify-overlay)"
    fi
    ;;
  Linux)
    if [ -n "${DISPLAY:-}" ]; then
      pass "DISPLAY=$DISPLAY"
    else
      echo "  WARN  DISPLAY unset (X11 overlay drive needs xvfb-run or a session)"
    fi
    for t in xdotool xprop xwininfo; do
      if command -v "$t" > /dev/null; then
        pass "$t on PATH"
      else
        echo "  WARN  $t missing (needed for verify-overlay-x11)"
      fi
    done
    if command -v xterm > /dev/null; then
      pass "xterm on PATH"
    else
      echo "  WARN  xterm missing (verify-overlay-x11 perch prop) — unit proof still OK"
    fi
    if [ -n "${DISPLAY:-}" ]; then
      if xprop -root _NET_SUPPORTING_WM_CHECK 2> /dev/null | grep -q 'window id'; then
        pass "supporting WM published"
      elif command -v openbox > /dev/null; then
        echo "  WARN  no supporting WM yet; openbox is installed (script can start it)"
      else
        echo "  WARN  no supporting WM and openbox not installed (Xvfb needs openbox) — overlay drive blocked"
      fi
    fi
    if ldconfig -p 2> /dev/null | grep -q ayatana-appindicator3 || [ -e /usr/lib/x86_64-linux-gnu/libayatana-appindicator3.so.1 ]; then
      pass "libayatana-appindicator3 present"
    else
      echo "  WARN  libayatana-appindicator3 missing — overlay panics on tray init (apt install libayatana-appindicator3-1)"
    fi
    ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT)
    pass "Windows host — use drive-overlay-win.ps1 / verify-settings-win.ps1"
    ;;
  *)
    echo "  WARN  unknown OS $OS"
    ;;
esac

if [ -n "${APP_PID:-}" ]; then
  if kill -0 "$APP_PID" 2> /dev/null; then
    pass "APP_PID=$APP_PID alive"
  else
    fail "APP_PID=$APP_PID not running"
  fi
fi

if [ "$UNITS" = "1" ]; then
  echo "doctor: running unit suites…"
  if cargo test -p ai-buddy-core -q; then
    pass "cargo test -p ai-buddy-core"
  else
    fail "cargo test -p ai-buddy-core"
  fi
  if node --test tests/*.test.js > /dev/null; then
    pass "node --test tests/*.test.js"
  else
    fail "node --test tests/*.test.js"
  fi
  if bash scripts/test_verify_overlay_diagnostics.sh > /dev/null; then
    pass "scripts/test_verify_overlay_diagnostics.sh"
  else
    fail "scripts/test_verify_overlay_diagnostics.sh"
  fi
fi

if [ "$fails" -eq 0 ]; then
  echo "doctor: OK"
  append_proof "doctor OK (units=$UNITS) evidence=$AI_BUDDY_VERIFY_EVIDENCE"
  exit 0
fi
echo "doctor: $fails failure(s)"
append_proof "doctor FAILED ($fails) — do not Drive until fixed"
exit 1
