#!/usr/bin/env bash
# Thin OS gate for PSScriptAnalyzer: skip on Linux/Darwin, run on Windows.
#
# PSScriptAnalyzer and pwsh are Windows-native. This gate lets Linux/macOS devs
# commit without installing pwsh. Windows CI enforces the check.

set -euo pipefail

case "$(uname -s)" in
  Linux | Darwin)
    echo "skipped - PSScriptAnalyzer runs on Windows" >&2
    exit 0
    ;;
  *)
    # Windows (MINGW/MSYS/CYGWIN/Windows_NT) or unknown: run the analyzer
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
    ;;
esac
