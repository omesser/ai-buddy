#!/usr/bin/env bash
# Shared by scripts/sync-mattpocock.sh and scripts/sync-pstack.sh. Each defines
# its own `EXCLUDED`, one `name|reason` per line, then sources this for the
# two helpers below so the two syncs cannot drift on how they read it.
set -euo pipefail

excluded_names() {
  printf '%s\n' "$EXCLUDED" | cut -d'|' -f1 | sort
}

# The `excluded` object body for the lock file, one JSON member per line.
excluded_json() {
  local first=1 name reason
  while IFS='|' read -r name reason; do
    [ -n "$name" ] || continue
    [ "$first" -eq 1 ] || echo ','
    printf '    "%s": "%s"' "$name" "$reason"
    first=0
  done <<< "$EXCLUDED"
  echo
}
