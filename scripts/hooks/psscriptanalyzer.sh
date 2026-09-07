#!/usr/bin/env bash
set -euo pipefail

if ! command -v pwsh >/dev/null 2>&1; then
  echo "skipped - pwsh not installed" >&2
  exit 0
fi

pwsh -NoProfile -Command 'if (-not (Get-Module -ListAvailable PSScriptAnalyzer)) { Install-Module PSScriptAnalyzer -Force -Scope CurrentUser }' >/dev/null 2>&1

for f in "$@"; do
  pwsh -NoProfile -Command "Invoke-ScriptAnalyzer -Path \"$f\" -Severity Error -EnableExit"
done
