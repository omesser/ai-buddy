#!/usr/bin/env pwsh
# Optional Windows console smoke for Chrome_WidgetWin_0 error 1411.
# Not acceptance. The quit plan, Ctrl+C inheritance probe, and harness
# shutdown tests are what CI runs. Do not treat this script as the gate.
#
# When a Windows console session is authorized:
#   $env:FIDGET_VERIFY_BIN = "target\debug\fidget.exe"
#   .\scripts\windows-ctrl-c-smoke.ps1
# Then press Ctrl+C in that console. The process should leave without
# "Failed to unregister class Chrome_WidgetWin_0" or "Error = 1411".

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
