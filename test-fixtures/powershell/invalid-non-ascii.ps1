#!/usr/bin/env pwsh
# Invalid script with non-ASCII characters
# This should FAIL the ASCII-only check

$ErrorActionPreference = "Stop"

# This comment contains an em-dash — which is UTF-8 non-ASCII
$result = "This script should fail — because of em-dash"

Write-Host $result
exit 0
