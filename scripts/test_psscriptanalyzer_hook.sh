#!/usr/bin/env bash
#
# Test the PSScriptAnalyzer pre-commit hook.
#
# Nothing else in the repository parses a .ps1, so this is the only check that
# fails if the hook's severity list stops admitting ParseError. #493.

set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

HOOK=scripts/hooks/psscriptanalyzer.sh

# The hook exits 0 and prints why when pwsh or the module is absent, which
# would read as "a broken script passed". Skip instead of asserting a lie.
if ! command -v pwsh > /dev/null 2>&1 ||
  ! pwsh -NoProfile -Command 'if (-not (Get-Module -ListAvailable PSScriptAnalyzer)) { exit 1 }' > /dev/null 2>&1; then
  echo "skipped - pwsh or PSScriptAnalyzer not installed"
  exit 0
fi

TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

# shellcheck disable=SC2016 # $x is PowerShell's variable, and stays literal.
printf 'if ($x {\n  Write-Host "hi"\n' > "$TEMP_DIR/broken.ps1"
printf 'Write-Host "hi"\n' > "$TEMP_DIR/clean.ps1"

echo "Test: a .ps1 that does not parse fails the hook"
if "$HOOK" "$TEMP_DIR/broken.ps1" > /dev/null 2>&1; then
  echo "  FAIL: hook exited 0 on a file with a parse error"
  exit 1
fi
echo "  PASS"

echo "Test: a .ps1 that parses passes the hook"
if ! "$HOOK" "$TEMP_DIR/clean.ps1" > /dev/null 2>&1; then
  echo "  FAIL: hook exited non-zero on a clean file"
  "$HOOK" "$TEMP_DIR/clean.ps1" || true
  exit 1
fi
echo "  PASS"

echo ""
echo "All PSScriptAnalyzer hook tests passed."
