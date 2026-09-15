#!/usr/bin/env bash
# Wrap scripts/verify-overlay.sh and copy artifacts into surviving evidence.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
# shellcheck disable=SC1091
. "$SCRIPT_DIR/common.sh"

cd "$REPO_ROOT"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "drive-overlay-macos: macOS only (got $(uname -s))" >&2
  append_proof "drive-overlay-macos SKIP — not Darwin"
  exit 2
fi

BEFORE="$(find .verify -maxdepth 1 -type d -regex '\.verify/[0-9].*' 2> /dev/null | sort || true)"

set +e
bash scripts/verify-overlay.sh
STATUS=$?
set -e

AFTER="$(find .verify -maxdepth 1 -type d -regex '\.verify/[0-9].*' 2> /dev/null | sort || true)"
NEW="$(comm -13 <(echo "$BEFORE") <(echo "$AFTER") || true)"
DEST="$AI_BUDDY_VERIFY_EVIDENCE/overlay-presence"
mkdir -p "$DEST"
if [ -n "$NEW" ]; then
  while IFS= read -r dir; do
    [ -n "$dir" ] || continue
    cp -a "$dir" "$DEST/"
  done <<< "$NEW"
else
  LATEST="$(find .verify -maxdepth 1 -mindepth 1 -type d ! -name 'x11-*' ! -name 'win-*' 2> /dev/null | sort | tail -1 || true)"
  [ -n "$LATEST" ] && cp -a "$LATEST" "$DEST/"
fi

if [ "$STATUS" -eq 0 ]; then
  append_proof "drive-overlay-macos PASS — evidence under $DEST"
else
  append_proof "drive-overlay-macos FAIL exit=$STATUS — see $DEST"
fi
exit "$STATUS"
