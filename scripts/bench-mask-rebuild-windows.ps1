#!/usr/bin/env pwsh
# Benchmark Windows mask rebuild cost for click-through regions (issue #428).
# Measures SetWindowRgn calls under different scenarios using shipped characters.
#
# Usage: scripts\bench-mask-rebuild-windows.ps1 [SCENARIO] [DURATION]
#
# Arguments are positional. SCENARIO defaults to 'idle', DURATION to 10 seconds.
#
# Scenarios:
#   idle       - BMO perched, cursor away (expect ~0 rebuilds/sec)
#   walk       - BMO walking under cursor (expect rebuilds at motion rate)
#   fast       - BMO react animation (10 fps) under cursor
#   large      - Black Mage at scale=3 (larger rendered sprite)

param(
    [string]$Scenario = "idle",
    [int]$Duration = 10
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path))

$bin = "target\debug\ai-buddy.exe"

function Show-Usage {
    Write-Host @"
Usage: .\scripts\bench-mask-rebuild-windows.ps1 [SCENARIO] [DURATION]

Scenarios:
  idle       BMO perched, cursor away (expect ~0 rebuilds/sec)
  walk       BMO walking under cursor (expect rebuilds at motion rate)
  fast       BMO react animation (10 fps) under cursor
  large      Black Mage at scale=3 (larger rendered sprite)

Duration: seconds to measure (default: 10)
"@
    exit 1
}

switch ($Scenario) {
    { $_ -in @("idle", "walk", "fast", "large") } { break }
    { $_ -in @("--help", "-h", "-?") } { Show-Usage }
    default {
        Write-Error "Unknown scenario: $Scenario"
        Show-Usage
    }
}

if (-not (Test-Path $bin)) {
    Write-Error "no $bin - run: cargo build --bin ai-buddy"
    exit 2
}

$log = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '-mask-rebuild.log'

Write-Host "Scenario: $Scenario"
Write-Host "Duration: ${Duration}s"
Write-Host "Log: $log"
Write-Host ""

# Set environment for tracing mask rebuilds
$env:AI_BUDDY_TRACE_MASK_REBUILD = "1"

# Choose character and setup based on scenario
switch ($Scenario) {
    "large" {
        $env:AI_BUDDY_INSTANCES = "Black Mage"
        Write-Host "Using Black Mage (scale=3, larger rendered sprite)"
    }
    default {
        $env:AI_BUDDY_INSTANCES = "BMO"
        Write-Host "Using BMO (126x128@1x)"
    }
}

# Start the app
$app = Start-Process -FilePath $bin -RedirectStandardOutput $log -RedirectStandardError $log -PassThru -WindowStyle Hidden

try {
    # Wait for overlays to be created
    Write-Host -NoNewline "Waiting for overlays..."
    $waited = 0
    $maxWait = 30
    while ($waited -lt $maxWait) {
        if (Select-String -Path $log -Pattern "overlay configured" -Quiet) {
            Write-Host " ready"
            break
        }
        Start-Sleep -Milliseconds 100
        $waited++
    }

    if ($waited -ge $maxWait) {
        Write-Error "Overlay did not start within ${maxWait}s"
        exit 3
    }

    # Clear any startup mask rebuilds
    Start-Sleep -Milliseconds 500
    Get-Content $log | Out-Null

    # Scenario-specific setup
    switch ($Scenario) {
        "walk" {
            Write-Host "Move cursor over BMO and keep it there for ${Duration}s..."
            Write-Host "BMO will walk under the cursor (hold Shift to prevent clicks)."
        }
        "fast" {
            Write-Host "Move cursor over BMO for ${Duration}s..."
            Write-Host "Right-click on BMO repeatedly to trigger React animation."
        }
        "large" {
            Write-Host "Move cursor over Black Mage for ${Duration}s..."
        }
        default {
            Write-Host "Keep cursor away from BMO for ${Duration}s..."
        }
    }

    $startTime = Get-Date
    # Wait for measurement duration
    Start-Sleep -Seconds $Duration

    # Stop the app
    Write-Host "Stopping app..."
    Stop-Process -Id $app.Id -Force
    $app.WaitForExit(5000) | Out-Null

    Write-Host ""
    Write-Host "=== Mask Rebuild Summary ==="

    # Parse the log for mask_rebuild lines
    $rebuilds = Select-String -Path $log -Pattern "mask_rebuild:" | ForEach-Object { $_.Line }

    if ($rebuilds.Count -eq 0) {
        Write-Host "No mask rebuilds detected (expected for 'idle' scenario)"
        Write-Host ""
        Write-Host "Full log at: $log"
        exit 0
    }

    # Parse timing data
    $times = @()
    $rebuilds | ForEach-Object {
        if ($_ -match '(\d+\.\d+) ms$') {
            $times += [double]$matches[1]
        }
    }

    $count = $times.Count
    $totalMs = ($times | Measure-Object -Sum).Sum
    $avgMs = if ($count -gt 0) { $totalMs / $count } else { 0 }
    $minMs = if ($count -gt 0) { ($times | Measure-Object -Minimum).Minimum } else { 0 }
    $maxMs = if ($count -gt 0) { ($times | Measure-Object -Maximum).Maximum } else { 0 }
    $rebuildsPerSec = $count / $Duration

    Write-Host "Rebuilds: $count"
    Write-Host "Total time: $($totalMs.ToString('F2')) ms"
    Write-Host "Average: $($avgMs.ToString('F2')) ms/rebuild"
    Write-Host "Min: $($minMs.ToString('F2')) ms"
    Write-Host "Max: $($maxMs.ToString('F2')) ms"
    Write-Host "Rate: $($rebuildsPerSec.ToString('F2')) rebuilds/sec"
    Write-Host ""

    # Show sample rebuild lines
    Write-Host "Sample rebuild lines:"
    $rebuilds | Select-Object -First 3 | ForEach-Object { Write-Host "  $_" }

    Write-Host ""
    Write-Host "Full log at: $log"
}
finally {
    # Clean up
    if ($app -and -not $app.HasExited) {
        Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
    }
}
