#!/usr/bin/env pwsh
# Valid ASCII-only PowerShell script
# This should pass the ASCII-only check

$ErrorActionPreference = "Stop"

function Test-ValidScript {
    param([string]$Message)
    Write-Host "Test message: $Message" -ForegroundColor Green
}

# Using standard ASCII hyphen (not em-dash)
$result = "This is a test - with regular hyphens"
Test-ValidScript -Message $result

exit 0
