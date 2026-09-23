#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

fail() {
  echo "[FAIL] $*" >&2
  if [ -n "${LOG:-}" ] && [ -f "$LOG" ]; then
    echo "[FAIL] last log lines:" >&2
    tail -30 "$LOG" >&2 || true
  fi
  exit 1
}

info() { echo "[INFO] $*"; }

[ -n "${DISPLAY:-}" ] || fail "DISPLAY is unset. Run under X11 or xvfb-run."
command -v xdotool > /dev/null || fail "xdotool not found"
command -v xwininfo > /dev/null || fail "xwininfo not found"
command -v xprop > /dev/null || fail "xprop not found"

BIN="${AI_BUDDY_VERIFY_BIN:-$ROOT/target/debug/ai-buddy}"
[ -x "$BIN" ] || fail "binary missing: $BIN (cargo build -p ai-buddy)"

STAMP=$(date +%Y%m%d-%H%M%S)
OUT="$ROOT/.verify/anchor-$STAMP"
mkdir -p "$OUT"
LOG="$OUT/app.log"

if xprop -root _NET_SUPPORTING_WM_CHECK 2> /dev/null | grep -q 'window id'; then
  fail "a window manager is running and will drag the anchor back on screen. Use xvfb-run with no WM."
fi

cleanup() {
  if [ -n "${APP_PID:-}" ]; then
    kill "$APP_PID" 2> /dev/null || true
  fi
}
trap cleanup EXIT

ROOT_W=$(xwininfo -root | awk '/^  Width:/ {print $2}')
ROOT_H=$(xwininfo -root | awk '/^  Height:/ {print $2}')
info "Display ${ROOT_W}x${ROOT_H}"

AI_BUDDY_TRACE_FRAMES=1 LIBGL_ALWAYS_SOFTWARE=1 "$BIN" > "$LOG" 2>&1 &
APP_PID=$!

for _ in $(seq 1 80); do
  grep -q '^overlay:' "$LOG" && break
  sleep 0.25
done
grep -q '^overlay:' "$LOG" || fail "app never published an overlay line"
kill -0 "$APP_PID" 2> /dev/null || fail "app exited during startup"
sleep 1

on_desktop() {
  local x="$1" y="$2" w="$3" h="$4"
  [ "$((x + w))" -gt 0 ] && [ "$x" -lt "$ROOT_W" ] &&
    [ "$((y + h))" -gt 0 ] && [ "$y" -lt "$ROOT_H" ]
}

fails=0
for id in $(xdotool search --class 'Ai-buddy' 2> /dev/null || true); do
  info_txt=$(xwininfo -id "$id" 2> /dev/null || true)
  map=$(printf '%s\n' "$info_txt" | awk '/Map State:/ {print $3}')
  [ "$map" = "IsViewable" ] || continue
  w=$(printf '%s\n' "$info_txt" | awk '/^  Width:/ {print $2}')
  h=$(printf '%s\n' "$info_txt" | awk '/^  Height:/ {print $2}')
  x=$(printf '%s\n' "$info_txt" | awk '/Absolute upper-left X:/ {print $4}')
  y=$(printf '%s\n' "$info_txt" | awk '/Absolute upper-left Y:/ {print $4}')
  name=$(xprop -id "$id" WM_NAME 2> /dev/null | sed -n 's/^WM_NAME(STRING) = "\(.*\)"/\1/p')
  info "window $id ${w}x${h} at ${x},${y} map=$map name=${name:-?}"
  on_desktop "$x" "$y" "$w" "$h" || continue
  if [ "$w" -ge $((ROOT_W * 8 / 10)) ] && [ "$h" -ge $((ROOT_H * 8 / 10)) ]; then
    continue
  fi
  if [ "$name" = "Settings" ]; then
    continue
  fi
  echo "[FAIL] on-desktop window $id ${w}x${h} at ${x},${y} (${name:-untitled}) is the anchor surface" >&2
  fails=1
done

[ "$fails" -eq 0 ] || exit 1
info "no anchor pixels on the desktop"
