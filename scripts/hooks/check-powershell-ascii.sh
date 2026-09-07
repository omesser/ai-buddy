#!/usr/bin/env bash
# ASCII-only gate for PowerShell scripts
#
# Rejects any .ps1 file containing non-ASCII bytes to prevent encoding issues
# like the em-dash footgun in #392 that broke Windows PowerShell 5.1.
#
# Usage: pre-commit hook, invoked automatically on staged .ps1 files

set -euo pipefail

exit_code=0

for file in "$@"; do
  if ! [ -f "$file" ]; then
    continue
  fi

  # Check for non-ASCII bytes (any byte > 127)
  if LC_ALL=C grep -n '[^ -~	]' "$file" >/dev/null 2>&1; then
    echo "ERROR: $file contains non-ASCII characters"
    echo "PowerShell scripts must be ASCII-only to avoid encoding issues on Windows PowerShell 5.1"
    echo "Non-ASCII lines:"
    LC_ALL=C grep -n '[^ -~	]' "$file" || true
    exit_code=1
  fi
done

exit $exit_code
