#!/usr/bin/env pwsh
# Windows anchor taskbar smoke test (#767)
#
# Verifies the Settings window does NOT auto-open on startup (Q1), and DOES
# open when the anchor HWND receives a WM_ACTIVATE with WA_CLICKACTIVE (Q2).
#
# Usage:
#   .\scripts\verify-anchor-taskbar-win.ps1
#   $env:AI_BUDDY_VERIFY_BIN="path\to\ai-buddy.exe" .\scripts\verify-anchor-taskbar-win.ps1
#
# Expects a built debug binary (does not cargo build). Logs under .verify/anchor-taskbar-win-<stamp>/.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not (Test-Path (Join-Path $Root "src-tauri"))) { $Root = (Get-Location).Path }
Set-Location $Root

function Info($m) { Write-Host "[INFO] $m" -ForegroundColor Green }
function Pass($m) { Write-Host "[PASS] $m" -ForegroundColor Green }
function Fail($m) {
  Write-Host "[FAIL] $m" -ForegroundColor Red
  if ($script:AppProc -and -not $script:AppProc.HasExited) {
    Stop-Process -Id $script:AppProc.Id -Force -ErrorAction SilentlyContinue
  }
  exit 1
}

$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$Out = Join-Path $Root ".verify\anchor-taskbar-win-$Stamp"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Info "Output: $Out"

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Collections.Generic;
public class AnchorVerify {
  public const int GWL_STYLE = -16;
  public const uint WS_VISIBLE = 0x10000000;
  public const uint WM_ACTIVATE = 0x0006;
  public const int WA_INACTIVE = 0;
  public const int WA_ACTIVE = 1;
  public const int WA_CLICKACTIVE = 2;
  [DllImport("user32.dll")] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hWnd, int nIndex);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool GetClassName(IntPtr hWnd, StringBuilder sb, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  public static List<IntPtr> FindWindowsByPid(uint pid) {
    var list = new List<IntPtr>();
    EnumWindows((h, l) => {
      uint wpid; GetWindowThreadProcessId(h, out wpid);
      if (wpid == pid && IsWindowVisible(h)) list.Add(h);
      return true;
    }, IntPtr.Zero);
    return list;
  }
}
"@

$Bin = if ($env:AI_BUDDY_VERIFY_BIN) { $env:AI_BUDDY_VERIFY_BIN } else { Join-Path $Root "target\debug\ai-buddy.exe" }
if (-not (Test-Path $Bin)) { Fail "missing $Bin - build with cargo first, or set AI_BUDDY_VERIFY_BIN" }
Pass "Binary ready: $Bin"

# Kill any existing ai-buddy processes
Get-Process -Name "ai-buddy" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# Launch ai-buddy without AI_BUDDY_OPEN_SETTINGS (should not auto-open Settings)
$env:AI_BUDDY_CHARACTER = "buddy-bot"
Remove-Item Env:AI_BUDDY_OPEN_SETTINGS -ErrorAction SilentlyContinue

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Bin
$psi.WorkingDirectory = $Root
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$script:AppProc = New-Object System.Diagnostics.Process
$script:AppProc.StartInfo = $psi
$null = $script:AppProc.Start()
$appPid = [uint32]$script:AppProc.Id
Info "Launched ai-buddy (PID=$appPid)"

# Wait ~8s for startup to settle
Start-Sleep -Milliseconds 8000
Info "Startup settled"

# Q1: Assert Settings window does NOT exist
function Find-SettingsHwnd($windows) {
  foreach ($hwnd in $windows) {
    $cls = New-Object System.Text.StringBuilder(256)
    $txt = New-Object System.Text.StringBuilder(256)
    [AnchorVerify]::GetClassName($hwnd, $cls, 256) | Out-Null
    [AnchorVerify]::GetWindowText($hwnd, $txt, 256) | Out-Null
    if ($cls.ToString() -eq "Tauri Window" -and $txt.ToString() -eq "Settings") {
      return $hwnd
    }
  }
  return [IntPtr]::Zero
}

$windows = [AnchorVerify]::FindWindowsByPid($appPid)
$settingsHwnd = Find-SettingsHwnd $windows

if ($settingsHwnd -ne [IntPtr]::Zero) {
  Fail "Q1 FAIL: Settings window auto-opened on startup (hwnd=$settingsHwnd)"
}
Pass "Q1 PASS: No Settings window auto-opened on startup"

# Q2: Find the anchor HWND (small window ~136x39 or width 50-200, height 20-80)
Info "Searching for anchor HWND (small window, same process)..."
$anchorHwnd = [IntPtr]::Zero
$windows = [AnchorVerify]::FindWindowsByPid($appPid)
foreach ($hwnd in $windows) {
  $rect = New-Object AnchorVerify+RECT
  if ([AnchorVerify]::GetWindowRect($hwnd, [ref]$rect)) {
    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top
    # Anchor is a tiny window: width 50-200, height 20-80
    if ($w -ge 50 -and $w -le 200 -and $h -ge 20 -and $h -le 80) {
      $sb = New-Object System.Text.StringBuilder(256)
      [AnchorVerify]::GetClassName($hwnd, $sb, 256) | Out-Null
      $className = $sb.ToString()
      Info "Found candidate anchor: hwnd=$hwnd, class=$className, size=${w}x${h}"
      $anchorHwnd = $hwnd
      break
    }
  }
}

if ($anchorHwnd -eq [IntPtr]::Zero) {
  Fail "Q2 FAIL: No anchor HWND found (expected small window 50-200w x 20-80h)"
}
Pass "Found anchor HWND: $anchorHwnd"

# Q2: Send WM_ACTIVATE with WA_CLICKACTIVE (2) to the anchor
Info "Sending PostMessageW(anchor, WM_ACTIVATE, WA_CLICKACTIVE=2, 0)..."
$wParam = [IntPtr]([AnchorVerify]::WA_CLICKACTIVE)
$result = [AnchorVerify]::PostMessage($anchorHwnd, [AnchorVerify]::WM_ACTIVATE, $wParam, [IntPtr]::Zero)
if (-not $result) {
  Fail "Q2 FAIL: PostMessage failed (GetLastError might tell why)"
}
Pass "PostMessage succeeded"

# Wait ~1s for Settings to open
Start-Sleep -Milliseconds 1000

# Assert Settings window now exists
$windows = [AnchorVerify]::FindWindowsByPid($appPid)
$settingsHwnd = Find-SettingsHwnd $windows

if ($settingsHwnd -eq [IntPtr]::Zero) {
  Fail "Q2 FAIL: Settings window did not open after PostMessage WM_ACTIVATE with WA_CLICKACTIVE"
}
Pass "Q2 PASS: Settings window opened after WM_ACTIVATE with WA_CLICKACTIVE"

# Clean up
if ($script:AppProc -and -not $script:AppProc.HasExited) {
  Stop-Process -Id $script:AppProc.Id -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "RESULT: Q1 PASS, Q2 PASS - $Out" -ForegroundColor Green
exit 0
