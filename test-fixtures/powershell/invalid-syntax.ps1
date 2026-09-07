#!/usr/bin/env pwsh
# Invalid script with syntax error
# This should FAIL the parse check

$ErrorActionPreference = "Stop"

# Missing closing brace
function Test-BadSyntax {
    param([string]$Message)
    Write-Host "This function has bad syntax"
# Missing closing brace here

exit 0
