#!/usr/bin/env pwsh
# Optional console smoke for Chrome_WidgetWin_0 error 1411; not the CI gate.
# Set $env:FIDGET_VERIFY_BIN, run it, press Ctrl+C, and expect no "Failed to
# unregister class Chrome_WidgetWin_0" or "Error = 1411".

$ErrorActionPreference = "Stop"

function Info($Message) { Write-Host "[INFO] $Message" }

$Bin = $env:FIDGET_VERIFY_BIN
if (-not $Bin) {
    Info "FIDGET_VERIFY_BIN is unset. Checklist only; nothing was launched."
    Info "1. Start fidget.exe from a console, not by double-click."
    Info "2. Wait until the character is on screen."
    Info "3. Press Ctrl+C once in that console."
    Info "4. Confirm the process exits and stderr has no Chrome_WidgetWin_0 / 1411."
    exit 0
}

if (-not (Test-Path -LiteralPath $Bin)) {
    Write-Error "FIDGET_VERIFY_BIN does not exist: $Bin"
}

Info "Starting $Bin. Press Ctrl+C in this console once the character is up."
Info "Pass: process exits, no Chrome_WidgetWin_0, no Error = 1411."
& $Bin
exit $LASTEXITCODE
