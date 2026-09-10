# Wrap scripts/verify-overlay-win.ps1 and copy artifacts into surviving evidence.
$ErrorActionPreference = "Stop"
$Helpers = Split-Path -Parent $MyInvocation.MyCommand.Path
$SkillRoot = Split-Path -Parent $Helpers
$RepoRoot = (Resolve-Path (Join-Path $SkillRoot "..\..\..")).Path
Set-Location $RepoRoot

if (-not $env:RUN_ID) { $env:RUN_ID = (Get-Date -Format "yyyyMMdd-HHmmss") + "-" + $PID }
if (-not $env:AI_BUDDY_VERIFY_ROOT) { $env:AI_BUDDY_VERIFY_ROOT = Join-Path $env:TEMP "ai-buddy-verify-$($env:RUN_ID)" }
if (-not $env:AI_BUDDY_VERIFY_EVIDENCE) { $env:AI_BUDDY_VERIFY_EVIDENCE = Join-Path $env:AI_BUDDY_VERIFY_ROOT "evidence" }
New-Item -ItemType Directory -Force -Path $env:AI_BUDDY_VERIFY_EVIDENCE | Out-Null

$Before = @()
if (Test-Path ".verify") {
  $Before = Get-ChildItem ".verify" -Directory -Filter "win-*" | ForEach-Object { $_.FullName }
}

& "$RepoRoot\scripts\verify-overlay-win.ps1"
$Status = $LASTEXITCODE

$Dest = Join-Path $env:AI_BUDDY_VERIFY_EVIDENCE "overlay-presence"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$After = @()
if (Test-Path ".verify") {
  $After = Get-ChildItem ".verify" -Directory -Filter "win-*" | ForEach-Object { $_.FullName }
}
foreach ($d in $After) {
  if ($Before -notcontains $d) {
    Copy-Item -Recurse -Force $d $Dest
  }
}

$Proof = Join-Path $env:AI_BUDDY_VERIFY_EVIDENCE "PROOF.md"
$Line = if ($Status -eq 0) { "drive-overlay-win PASS - evidence under $Dest" } else { "drive-overlay-win FAIL exit=$Status - see $Dest" }
Add-Content -Path $Proof -Value ("## " + (Get-Date).ToUniversalTime().ToString("o") + " UTC`n$Line`n")
exit $Status
