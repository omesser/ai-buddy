#!/usr/bin/env pwsh
# Windows anchor taskbar smoke: Settings stays closed at startup (Q1) and opens
# on a WA_CLICKACTIVE to the anchor (Q2). Needs a built binary; override with
# $env:FIDGET_VERIFY_BIN. Logs under .verify/anchor-taskbar-win-<stamp>/.

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

$Bin = if ($env:FIDGET_VERIFY_BIN) { $env:FIDGET_VERIFY_BIN } else { Join-Path $Root "target\debug\fidget.exe" }
if (-not (Test-Path $Bin)) { Fail "missing $Bin - build with cargo first, or set FIDGET_VERIFY_BIN" }
Pass "Binary ready: $Bin"

Get-Process -Name "fidget" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# FIDGET_OPEN_SETTINGS would open Settings on startup and void Q1.
$env:FIDGET_CHARACTER = "buddy-bot"
Remove-Item Env:FIDGET_OPEN_SETTINGS -ErrorAction SilentlyContinue

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Bin
$psi.WorkingDirectory = $Root
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$script:AppProc = New-Object System.Diagnostics.Process
$script:AppProc.StartInfo = $psi
$null = $script:AppProc.Start()
$appPid = [uint32]$script:AppProc.Id
Info "Launched fidget (PID=$appPid)"

Start-Sleep -Milliseconds 8000
Info "Startup settled"

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

Info "Searching for anchor HWND (tiny window title=Fidget, same process)..."
$anchorHwnd = [IntPtr]::Zero
$windows = [AnchorVerify]::FindWindowsByPid($appPid)
foreach ($hwnd in $windows) {
  $cls = New-Object System.Text.StringBuilder(256)
  $txt = New-Object System.Text.StringBuilder(256)
  [AnchorVerify]::GetClassName($hwnd, $cls, 256) | Out-Null
  [AnchorVerify]::GetWindowText($hwnd, $txt, 256) | Out-Null
  $className = $cls.ToString()
  $title = $txt.ToString()
  if ($title -ne "Fidget" -or $className -ne "Tauri Window") { continue }
  $rect = New-Object AnchorVerify+RECT
  if ([AnchorVerify]::GetWindowRect($hwnd, [ref]$rect)) {
    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top
    if ($w -ge 1 -and $w -le 20 -and $h -ge 1 -and $h -le 20) {
      Info "Found anchor: hwnd=$hwnd, class=$className, title=$title, size=${w}x${h}"
      $anchorHwnd = $hwnd
      break
    }
  }
}

if ($anchorHwnd -eq [IntPtr]::Zero) {
  Fail "Q2 FAIL: No anchor HWND found (expected title=Fidget, class=Tauri Window, size 1-20 x 1-20)"
}
Pass "Found anchor HWND: $anchorHwnd"

Info "Sending PostMessageW(anchor, WM_ACTIVATE, WA_CLICKACTIVE=2, 0)..."
$wParam = [IntPtr]([AnchorVerify]::WA_CLICKACTIVE)
$result = [AnchorVerify]::PostMessage($anchorHwnd, [AnchorVerify]::WM_ACTIVATE, $wParam, [IntPtr]::Zero)
if (-not $result) {
  Fail "Q2 FAIL: PostMessage failed (GetLastError might tell why)"
}
Pass "PostMessage succeeded"

Start-Sleep -Milliseconds 1000

$windows = [AnchorVerify]::FindWindowsByPid($appPid)
$settingsHwnd = Find-SettingsHwnd $windows

if ($settingsHwnd -eq [IntPtr]::Zero) {
  Fail "Q2 FAIL: Settings window did not open after PostMessage WM_ACTIVATE with WA_CLICKACTIVE"
}
Pass "Q2 PASS: Settings window opened after WM_ACTIVATE with WA_CLICKACTIVE"

if ($script:AppProc -and -not $script:AppProc.HasExited) {
  Stop-Process -Id $script:AppProc.Id -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "RESULT: Q1 PASS, Q2 PASS - $Out" -ForegroundColor Green
exit 0
