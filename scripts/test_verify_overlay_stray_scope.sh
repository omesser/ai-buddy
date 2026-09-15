#!/usr/bin/env bash
#
# Regression guard for #730: verify-overlay.sh must never signal a process by
# the "target/debug/ai-buddy" path suffix every worktree's binary shares. It
# scopes to this checkout's exact absolute binary path instead, the shape
# crates/verify/src/gesture.rs already uses for its stray_pid check.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

FAILED=0

echo "Test: no bare path-suffix pkill left in the script"
BARE=$(grep -c 'pkill -f .*target/debug/ai-buddy' scripts/verify-overlay.sh)
if [ "$BARE" = "0" ]; then
  echo "  PASS  no 'pkill -f .../target/debug/ai-buddy' left (it would match every worktree)"
else
  echo "  FAIL  $BARE such line(s) still present"
  FAILED=1
fi

echo
echo "Test: the EXIT/INT/TERM trap kills its own recorded pid, not a path pattern"
TRAP_LINE=$(grep -A2 "^trap 'pkill -f perch-window.swift" scripts/verify-overlay.sh | tr '\n' ' ')
# shellcheck disable=SC2016  # literal text to grep for, not an expansion
if echo "$TRAP_LINE" | grep -q 'kill "\$APP_PID"'; then
  echo "  PASS  trap kills \$APP_PID"
else
  echo "  FAIL  trap does not target \$APP_PID: $TRAP_LINE"
  FAILED=1
fi

echo
echo "Test: stray_pid() matches only this checkout's exact binary path"
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

sed -n '/^stray_pid()/p' scripts/verify-overlay.sh > "$TEMP_DIR/function.sh"
# shellcheck disable=SC1091
. "$TEMP_DIR/function.sh" 2> /dev/null

if ! type stray_pid > /dev/null 2>&1; then
  echo "  FAIL  scripts/verify-overlay.sh defines no stray_pid() to extract"
  FAILED=1
else
  FOREIGN_DIR="$TEMP_DIR/other-worktree/target/debug"
  mkdir -p "$FOREIGN_DIR"
  cat > "$FOREIGN_DIR/ai-buddy" << 'EOF'
#!/bin/sh
exec sleep 30
EOF
  chmod +x "$FOREIGN_DIR/ai-buddy"
  "$FOREIGN_DIR/ai-buddy" &
  FOREIGN_PID=$!
  sleep 0.2

  # shellcheck disable=SC2034  # read by the sourced stray_pid()
  BIN_PATH="$(pwd)/target/debug/ai-buddy"
  FOUND=$(stray_pid)
  if [ "$FOUND" = "$FOREIGN_PID" ]; then
    echo "  FAIL  stray_pid() matched a foreign worktree's process ($FOREIGN_PID) by path suffix"
    FAILED=1
  else
    echo "  PASS  stray_pid() left a foreign worktree's process ($FOREIGN_PID) unmatched"
  fi
  kill "$FOREIGN_PID" 2> /dev/null
fi

echo
if [ "$FAILED" = "0" ]; then
  echo "All stray-scope tests passed."
else
  echo "Some stray-scope tests FAILED."
  exit 1
fi
