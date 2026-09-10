#!/usr/bin/env bash
# Wrap scripts/verify-overlay-x11.sh and copy artifacts into surviving evidence.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
# shellcheck disable=SC1091
. "$SCRIPT_DIR/common.sh"

cd "$REPO_ROOT"

if [ -n "${AI_BUDDY_VERIFY_PREFIX:-}" ]; then
  export PATH="$AI_BUDDY_VERIFY_PREFIX/usr/bin:$PATH"
  export LD_LIBRARY_PATH="$AI_BUDDY_VERIFY_PREFIX/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  export XDG_DATA_DIRS="$AI_BUDDY_VERIFY_PREFIX/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
  export XDG_CONFIG_DIRS="$AI_BUDDY_VERIFY_PREFIX/etc/xdg:${XDG_CONFIG_DIRS:-/etc/xdg}"
  echo "drive-overlay-x11: using AI_BUDDY_VERIFY_PREFIX=$AI_BUDDY_VERIFY_PREFIX"
fi

if [ -z "${DISPLAY:-}" ]; then
  echo "drive-overlay-x11: DISPLAY unset. Use: xvfb-run -a -s \"-screen 0 1280x720x24\" $0" >&2
  append_proof "drive-overlay-x11 SKIP — DISPLAY unset"
  exit 2
fi

for t in xdotool xprop xwininfo; do
  command -v "$t" > /dev/null || {
    echo "drive-overlay-x11: missing $t" >&2
    append_proof "drive-overlay-x11 SKIP — missing $t"
    exit 2
  }
done

# verify-overlay-x11.sh requires `xterm` on PATH. Prefer real xterm; otherwise
# shim via xfce4-terminal (same geometry/title/-e flags the script passes).
if ! command -v xterm > /dev/null; then
  if command -v xfce4-terminal > /dev/null; then
    SHIM_BIN="$AI_BUDDY_VERIFY_SCRATCH/bin"
    mkdir -p "$SHIM_BIN"
    ln -sfn "$SCRIPT_DIR/xterm-shim.sh" "$SHIM_BIN/xterm"
    export PATH="$SHIM_BIN:$PATH"
    echo "drive-overlay-x11: using xfce4-terminal via xterm shim"
  else
    echo "drive-overlay-x11: missing xterm (and no xfce4-terminal to shim)" >&2
    append_proof "drive-overlay-x11 SKIP — missing xterm"
    exit 2
  fi
fi

BEFORE="$(find .verify -maxdepth 1 -type d -name 'x11-*' 2> /dev/null | sort || true)"

set +e
bash scripts/verify-overlay-x11.sh
STATUS=$?
set -e

AFTER="$(find .verify -maxdepth 1 -type d -name 'x11-*' 2> /dev/null | sort || true)"
NEW="$(comm -13 <(echo "$BEFORE") <(echo "$AFTER") || true)"
DEST="$AI_BUDDY_VERIFY_EVIDENCE/overlay-presence"
mkdir -p "$DEST"
if [ -n "$NEW" ]; then
  while IFS= read -r dir; do
    [ -n "$dir" ] || continue
    cp -a "$dir" "$DEST/"
    echo "drive-overlay-x11: copied $dir → $DEST/"
  done <<< "$NEW"
else
  # Fallback: newest stamp
  LATEST="$(find .verify -maxdepth 1 -type d -name 'x11-*' 2> /dev/null | sort | tail -1 || true)"
  if [ -n "$LATEST" ]; then
    cp -a "$LATEST" "$DEST/"
    echo "drive-overlay-x11: copied latest $LATEST → $DEST/"
  fi
fi

if [ "$STATUS" -eq 0 ]; then
  append_proof "drive-overlay-x11 PASS — feature overlay-presence (+ poke) evidence under $DEST"
else
  append_proof "drive-overlay-x11 FAIL exit=$STATUS — see $DEST (GUI gap or script failure)"
fi
exit "$STATUS"
