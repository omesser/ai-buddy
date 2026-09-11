# Sample the resident set of a running ai-buddy, Windows only.
#
# RSS on Windows lives in the main process and in WebView2 helper processes.
# WebView2 is Chromium-based (Edge), so it spawns multiple processes: GPU,
# Network, Renderer (one per webview), etc. This script discovers all
# msedgewebview2.exe processes after launch and attributes them to the app.
#
# Usage: scripts\bench-rss.ps1 [-Settle N] [-Seconds N] [-Interval N] [-Out FILE] [-Research]
#   Launches target\debug\ai-buddy.exe, waits Settle seconds, then samples every
#   Interval for Seconds, writes one TSV row per sample, prints min/median/max
#   over the sampled window and each process's peak working set, then stops the
#   app.
#
#   DEFAULT: Brief smoke test (settle ~3s, sample ~10s) — enough for fast
#   verification in a test matrix. Not a research soak.
#
#   -Research: Long research mode (settle 300s, sample 300s) for bathtub
#   curve analysis. The macOS script found a launch peak near twice steady
#   state, settling by ~5 minutes. Use this for measurement studies, not for
#   everyday verification.
#
#   Environment reaches the app unchanged, which is how a scenario is chosen:
#   AI_BUDDY_INSTANCES picks the roster, AI_BUDDY_CHARACTERS the packages.
#   Set HOME to a scratch directory to keep the real install's settings and
#   Action Log out of it.
#
# WorkingSet alone does not compare two runs on a busy machine. PeakWorkingSet
# only ever rises, so it survives noise. Both are reported. Compare scenarios
# on PeakWorkingSet and read the WorkingSet series for shape.
#
# Nothing here is a benchmark on its own: an RSS figure means nothing without
# the roster, the display count, and what the sprite was doing. Record those
# beside the number.

param(
    [int]$Settle = 3,
    [int]$Seconds = 10,
    [int]$Interval = 2,
    [string]$Out = "",
    [switch]$Research
)

if ($Research) {
    $Settle = 300
    $Seconds = 300
    $Interval = 5
}

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

$bin = "target\debug\ai-buddy.exe"
if (-not (Test-Path $bin)) {
    Write-Error "no $bin - run: cd src-tauri; cargo build --bin ai-buddy"
    exit 2
}

if ($Out -eq "") {
    $Out = [System.IO.Path]::GetTempFileName() -replace '\.tmp$', '.tsv'
}
$log = "$Out.app.log"

# Note WebView2 processes before launch. msedgewebview2.exe is the helper.
$beforeEdge = Get-Process -Name "msedgewebview2" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id

# Launch the app, merging stderr into stdout (overlay signal is on stderr)
$process = Start-Process -FilePath ".\$bin" -PassThru -RedirectStandardOutput $log -RedirectStandardError $log -WindowStyle Hidden
$app = $process.Id

# Cleanup function
function Stop-App {
    try {
        Stop-Process -Id $app -Force -ErrorAction SilentlyContinue
    } catch {}
}
Register-EngineEvent -SourceIdentifier PowerShell.Exiting -Action { Stop-App } | Out-Null
trap { Stop-App; throw }

# Wait for overlay initialization (signal is on stderr, now merged into $log)
$overlayReported = $false
$displays = 0
for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 1
    if (Test-Path $log) {
        $content = Get-Content $log -ErrorAction SilentlyContinue
        $match = $content | Select-String -Pattern 'overlay: (\d+) display'
        if ($match) {
            $displays = [int]$match.Matches[0].Groups[1].Value
            $overlayReported = $true
            break
        }
        # Check for startup failures
        if ($content -match "error|failed|cannot") {
            Write-Error "app failed to start; see $log"
            Get-Content $log
            exit 1
        }
    }
}

if (-not $overlayReported) {
    Write-Error "the app never reported its overlays; see $log"
    exit 1
}

# Find all msedgewebview2 processes that appeared after launch
Start-Sleep -Seconds 2  # Give helpers time to spawn
$afterEdge = Get-Process -Name "msedgewebview2" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id
$edgeHelpers = $afterEdge | Where-Object { $_ -notin $beforeEdge }

# Combine main process and helpers
$pids = @($app) + $edgeHelpers
$pidCount = $pids.Count

Write-Host "displays: $displays   main pid: $app   total processes: $pidCount"
Write-Host "pids: $($pids -join ' ')"
Write-Host "settling ${Settle}s, then sampling ${Seconds}s every ${Interval}s -> $Out"

# Log process tree for forensics
foreach ($procId in $pids) {
    try {
        $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
        Write-Host "  $procId $($proc.ProcessName) $($proc.WorkingSet64 / 1MB) MB"
    } catch {}
}

Start-Sleep -Seconds $Settle

# Write TSV header
$header = "epoch`ttotal_kb`t$($pids -join "`t")"
$header | Out-File -FilePath $Out -Encoding UTF8

$endTime = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $endTime) {
    $rss = @()
    foreach ($procId in $pids) {
        try {
            $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
            $rss += [math]::Round($proc.WorkingSet64 / 1KB)
        } catch {
            $rss += 0
        }
    }
    $total = ($rss | Measure-Object -Sum).Sum
    $epoch = [int][double]::Parse((Get-Date -UFormat %s))
    "$epoch`t$total`t$($rss -join "`t")" | Out-File -FilePath $Out -Append -Encoding UTF8
    Start-Sleep -Seconds $Interval
}

# Calculate statistics
$data = Import-Csv -Path $Out -Delimiter "`t" | Select-Object -Skip 0
$totals = $data | ForEach-Object { [int]$_.total_kb }
$sorted = $totals | Sort-Object
$min = [math]::Round($sorted[0] / 1024)
$median = [math]::Round($sorted[[math]::Floor($sorted.Count / 2)] / 1024)
$max = [math]::Round($sorted[-1] / 1024)

Write-Host "`ntotal   samples: $($sorted.Count)   min: $min MB   median: $median MB   max: $max MB"

# Per-process statistics
$column = 0
foreach ($procId in $pids) {
    try {
        $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
        $procName = $proc.ProcessName
        $peakWS = [math]::Round($proc.PeakWorkingSet64 / 1MB)

        # Calculate median RSS from TSV
        $pidRss = $data | ForEach-Object {
            $row = $_ | Get-Member -MemberType NoteProperty | Select-Object -ExpandProperty Name
            $colName = $row[$column + 2]  # Skip epoch and total_kb
            if ($colName) { [int]$_.$colName } else { 0 }
        }
        $pidRssSorted = $pidRss | Sort-Object
        $pidMedian = [math]::Round($pidRssSorted[[math]::Floor($pidRssSorted.Count / 2)] / 1024)

        Write-Host ("  {0,-6} {1,-28} rss median: {2,4} MB   peak working set: {3} MB" -f $procId, $procName, $pidMedian, $peakWS)
    } catch {
        Write-Host "  $procId gone"
    }
    $column++
}

Stop-App

Write-Host "`nLog written to: $log"
Write-Host "TSV written to: $Out"
