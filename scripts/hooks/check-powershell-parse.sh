#!/usr/bin/env bash
# PowerShell parse check
#
# Validates PowerShell syntax using Parser::ParseFile without executing scripts.
# Requires pwsh (PowerShell Core) to be available.
#
# Usage: pre-commit hook, invoked automatically on staged .ps1 files

set -euo pipefail

# Skip if pwsh is not available
if ! command -v pwsh > /dev/null 2>&1; then
  echo "SKIP: pwsh not available, skipping PowerShell parse check"
  exit 0
fi

exit_code=0

for file in "$@"; do
  if ! [ -f "$file" ]; then
    continue
  fi

  # Use PowerShell Parser::ParseFile to validate syntax
  # This catches syntax errors without executing the script
  if ! pwsh -NoProfile -Command "
    \$errors = @()
    \$null = [System.Management.Automation.Language.Parser]::ParseFile('$file', [ref]\$null, [ref]\$errors)
    if (\$errors.Count -gt 0) {
      Write-Host 'ERROR: Syntax errors in $file:'
      \$errors | ForEach-Object { Write-Host \"  Line \$(\$_.Extent.StartLineNumber): \$(\$_.Message)\" }
      exit 1
    }
  "; then
    exit_code=1
  fi
done

exit $exit_code
