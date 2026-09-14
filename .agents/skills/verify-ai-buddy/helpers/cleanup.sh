#!/usr/bin/env bash
# Tear down helper-owned processes and scratch. Never delete evidence.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
# shellcheck disable=SC1091
. "$SCRIPT_DIR/common.sh"

PIDLIST="$AI_BUDDY_VERIFY_SCRATCH/pids/owned.pids"
if [ -f "$PIDLIST" ]; then
  while read -r pid; do
    [ -n "$pid" ] || continue
    if kill -0 "$pid" 2> /dev/null; then
      echo "cleanup: kill $pid"
      kill "$pid" 2> /dev/null || true
      sleep 0.5
      kill -9 "$pid" 2> /dev/null || true
    fi
  done < "$PIDLIST"
fi

# Also clear single app.pid if present
if [ -f "$AI_BUDDY_VERIFY_SCRATCH/pids/app.pid" ]; then
  pid="$(cat "$AI_BUDDY_VERIFY_SCRATCH/pids/app.pid")"
  if kill -0 "$pid" 2> /dev/null; then
    echo "cleanup: kill app.pid $pid"
    kill "$pid" 2> /dev/null || true
    sleep 0.5
    kill -9 "$pid" 2> /dev/null || true
  fi
fi

rm -rf "$AI_BUDDY_VERIFY_SCRATCH"
mkdir -p "$AI_BUDDY_VERIFY_EVIDENCE"

echo "cleanup: scratch removed; evidence preserved at $AI_BUDDY_VERIFY_EVIDENCE"
append_proof "cleanup done; evidence still at $AI_BUDDY_VERIFY_EVIDENCE"
ls -la "$AI_BUDDY_VERIFY_EVIDENCE"
test -d "$AI_BUDDY_VERIFY_EVIDENCE"
