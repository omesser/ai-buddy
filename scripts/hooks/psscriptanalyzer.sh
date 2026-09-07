#!/usr/bin/env bash
set -euo pipefail

if ! command -v pwsh > /dev/null 2>&1; then
  echo "skipped - pwsh not installed" >&2
  exit 0
fi

# Skip rather than install. This runs on `git commit`, and installing a module
# into the committer's PowerShell profile is more than a lint should do behind
# their back. ubuntu-latest ships PSScriptAnalyzer, so CI is unaffected.
if ! pwsh -NoProfile -Command 'if (-not (Get-Module -ListAvailable PSScriptAnalyzer)) { exit 1 }' > /dev/null 2>&1; then
  echo "skipped - PSScriptAnalyzer not installed" >&2
  exit 0
fi

for f in "$@"; do
  pwsh -NoProfile -Command "Invoke-ScriptAnalyzer -Path \"$f\" -Severity Error -EnableExit"
done
