#!/usr/bin/env bash
# verify-overlay.sh must never signal a process by the "target/debug/ai-buddy"
# path suffix every worktree's binary shares. It scopes to this checkout's exact
# absolute binary path, the shape crates/verify/src/gesture.rs uses.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

FAILED=0
FOREIGN_PID=""
OWN_PID=""
FOREIGN_HOLD=""
OWN_HOLD=""

cleanup() {
  if [ -n "$FOREIGN_HOLD" ]; then
    eval "exec $FOREIGN_HOLD>&-" 2> /dev/null || true
  fi
  if [ -n "$OWN_HOLD" ]; then
    eval "exec $OWN_HOLD>&-" 2> /dev/null || true
  fi
  if [ -n "$FOREIGN_PID" ]; then
    kill "$FOREIGN_PID" 2> /dev/null || true
    wait "$FOREIGN_PID" 2> /dev/null || true
  fi
  if [ -n "$OWN_PID" ]; then
    kill "$OWN_PID" 2> /dev/null || true
    wait "$OWN_PID" 2> /dev/null || true
  fi
  rm -rf "$TEMP_DIR"
}

TEMP_DIR=$(mktemp -d)
trap cleanup EXIT

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
# Stay alive with this script path still in argv until stdin closes.
read -r _ || true
EOF
  chmod +x "$FOREIGN_DIR/ai-buddy"
  
  FOREIGN_FIFO="$TEMP_DIR/foreign.fifo"
  mkfifo "$FOREIGN_FIFO"
  "$FOREIGN_DIR/ai-buddy" < "$FOREIGN_FIFO" &
  FOREIGN_PID=$!
  exec {FOREIGN_HOLD}> "$FOREIGN_FIFO"
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
  exec {FOREIGN_HOLD}>&-
  wait "$FOREIGN_PID" 2> /dev/null || true
  kill "$FOREIGN_PID" 2> /dev/null || true
  FOREIGN_HOLD=""
  FOREIGN_PID=""

  # The negative above passes even if stray_pid() can never match anything.
  # This is the half that fails when the script launches the app by a relative
  # ./target/debug/ai-buddy: argv then holds no absolute path for pgrep to see.
  OWN_DIR="$TEMP_DIR/this-checkout/target/debug"
  mkdir -p "$OWN_DIR"
  cat > "$OWN_DIR/ai-buddy" << 'EOF'
#!/bin/sh
# Stay alive with this script path still in argv until stdin closes.
read -r _ || true
EOF
  chmod +x "$OWN_DIR/ai-buddy"
  
  OWN_FIFO="$TEMP_DIR/own.fifo"
  mkfifo "$OWN_FIFO"
  BIN_PATH="$OWN_DIR/ai-buddy"
  "$BIN_PATH" < "$OWN_FIFO" &
  OWN_PID=$!
  exec {OWN_HOLD}> "$OWN_FIFO"
  sleep 0.2

  FOUND=$(stray_pid)
  if [ -n "$FOUND" ]; then
    echo "  PASS  stray_pid() found a stray at its own BIN_PATH (pid $FOUND)"
  else
    echo "  FAIL  stray_pid() missed a stray at its own BIN_PATH (pid $OWN_PID)"
    FAILED=1
  fi
  exec {OWN_HOLD}>&-
  wait "$OWN_PID" 2> /dev/null || true
  kill "$OWN_PID" 2> /dev/null || true
  OWN_HOLD=""
  OWN_PID=""
fi

echo
echo "Test: the app is launched through \$BIN_PATH, so argv carries the absolute path"
if grep -qE '^\s*(AI_BUDDY_[A-Z_]+=1 )*\./target/debug/ai-buddy' scripts/verify-overlay.sh; then
  echo "  FAIL  a launch still uses ./target/debug/ai-buddy; stray_pid() cannot see a relative argv"
  FAILED=1
else
  echo "  PASS  no relative launch left"
fi

echo
if [ "$FAILED" = "0" ]; then
  echo "All stray-scope tests passed."
else
  echo "Some stray-scope tests FAILED."
  exit 1
fi
