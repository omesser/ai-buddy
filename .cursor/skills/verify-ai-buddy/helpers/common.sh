#!/usr/bin/env bash
# Shared paths for verify-ai-buddy helpers. Source from repo-relative helpers.
set -euo pipefail

_helpers_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AI_BUDDY_SKILL_ROOT="$(cd "$_helpers_dir/.." && pwd)"
REPO_ROOT="$(cd "$_helpers_dir/../../../.." && pwd)"
# helpers -> verify-ai-buddy -> skills -> .cursor -> repo
# From helpers: .. = skill, ../.. = skills, ../../.. = .cursor, ../../../.. = repo
if [ ! -f "$REPO_ROOT/Cargo.toml" ] || [ ! -d "$REPO_ROOT/src-tauri" ]; then
  # Fallback: walk up looking for workspace
  REPO_ROOT="$(cd "$_helpers_dir" && while [ ! -f Cargo.toml ] || [ ! -d src-tauri ]; do
    cd .. || exit 1
    [ "$PWD" = "/" ] && exit 1
  done && pwd)"
fi

export REPO_ROOT
export AI_BUDDY_SKILL_ROOT

export RUN_ID="${RUN_ID:-$(date +%Y%m%d-%H%M%S)-$$}"
export AI_BUDDY_VERIFY_ROOT="${AI_BUDDY_VERIFY_ROOT:-/tmp/ai-buddy-verify-$RUN_ID}"
export AI_BUDDY_VERIFY_EVIDENCE="${AI_BUDDY_VERIFY_EVIDENCE:-$AI_BUDDY_VERIFY_ROOT/evidence}"
export AI_BUDDY_VERIFY_SCRATCH="${AI_BUDDY_VERIFY_SCRATCH:-$AI_BUDDY_VERIFY_ROOT/scratch}"

mkdir -p "$AI_BUDDY_VERIFY_EVIDENCE" "$AI_BUDDY_VERIFY_SCRATCH/pids"

ai_buddy_bin() {
  if [ -x "$REPO_ROOT/target/release/ai-buddy" ]; then
    echo "$REPO_ROOT/target/release/ai-buddy"
  elif [ -x "$REPO_ROOT/target/debug/ai-buddy" ]; then
    echo "$REPO_ROOT/target/debug/ai-buddy"
  else
    return 1
  fi
}

record_pid() {
  local pid="$1"
  echo "$pid" >> "$AI_BUDDY_VERIFY_SCRATCH/pids/owned.pids"
}

append_proof() {
  local line="$1"
  mkdir -p "$AI_BUDDY_VERIFY_EVIDENCE"
  {
    echo "## $(date -u +%Y-%m-%dT%H:%M:%SZ) UTC"
    echo "$line"
    echo
  } >> "$AI_BUDDY_VERIFY_EVIDENCE/PROOF.md"
}
