#!/usr/bin/env bash
set -euo pipefail

if ! command -v pwsh >/dev/null 2>&1; then
  echo "skipped - pwsh not installed" >&2
  exit 0
fi

>&2 echo "running PSScriptAnalyzer on: $*"

pwsh -NoProfile -Command "
  if (!(Get-Module -ListAvailable PSScriptAnalyzer)) {
    Install-Module -Name PSScriptAnalyzer -Force -Scope CurrentUser
  }
  foreach (\$file in \$args) {
    \$results = Invoke-ScriptAnalyzer -Path \$file -Severity Error
    if (\$results) {
      \$results | Format-Table -AutoSize
      exit 1
    }
  }
" "$@"
