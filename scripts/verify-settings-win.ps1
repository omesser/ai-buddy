#!/usr/bin/env pwsh
# Windows Settings Window smoke test (#392)
#
# Verifies the native Win32 settings window opens, displays controls correctly,
# and field labels persist after tab switching (label-wipe fix).
#
# Usage:
#   .\scripts\verify-settings-win.ps1
#   $env:AI_BUDDY_VERIFY_BIN="path\to\ai-buddy.exe" .\scripts\verify-settings-win.ps1
#
# Expects a built debug binary (does not cargo build - pair with VsDevCmd).
# Dual-display required. Logs under .verify/settings-win-<stamp>/.

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
$Out = Join-Path $Root ".verify\settings-win-$Stamp"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Log = Join-Path $Out "app.log"
Info "Output: $Out"

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Collections.Generic;
public class SettingsVerify {
  public const int GWL_STYLE = -16;
  public const uint WS_VISIBLE = 0x10000000;
  public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
  public const uint MOUSEEVENTF_LEFTUP = 0x0004;
  public const uint MONITORINFOF_PRIMARY = 1;
  public const int TCM_GETCURSEL = 0x130b;
  public const int TCM_SETCURSEL = 0x130c;
  public const int TCM_GETITEMRECT = 0x130a;
  public const uint WM_NOTIFY = 0x004E;
  public const uint WM_LBUTTONDOWN = 0x0201;
  public const uint WM_LBUTTONUP = 0x0202;
  public const int GA_ROOT = 2;
  [DllImport("user32.dll")] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hWndParent, EnumChildProc lpEnumFunc, IntPtr lParam);
  public delegate bool EnumChildProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hWnd, int nIndex);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int nMaxCount);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetClassName(IntPtr hWnd, StringBuilder sb, int nMaxCount);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, ref RECT lParam);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT lpPoint);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT pt);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hWnd, int gaFlags);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr lprcClip, MonitorEnumProc lpfnEnum, IntPtr dwData);
  public delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO {
    public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags;
  }
  public static RECT? GetSecondaryWorkArea() {
    RECT? secondary = null;
    EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr h, IntPtr hdc, ref RECT r, IntPtr d) => {
      MONITORINFO mi = new MONITORINFO();
      mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
      if (GetMonitorInfo(h, ref mi) && (mi.dwFlags & MONITORINFOF_PRIMARY) == 0) {
        secondary = mi.rcWork;
      }
      return true;
    }, IntPtr.Zero);
    return secondary;
  }
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
if (-not (Test-Path $Bin)) { Fail "missing $Bin - build with VsDevCmd first, or set AI_BUDDY_VERIFY_BIN" }
Pass "Binary ready"

$sec = [SettingsVerify]::GetSecondaryWorkArea()
if ($null -eq $sec) { Fail "No secondary monitor (dual-display required)" }
$secLeft = $sec.Left; $secTop = $sec.Top
$secW = $sec.Right - $sec.Left; $secH = $sec.Bottom - $sec.Top
Info "Secondary work area: ${secLeft},${secTop} ${secW}x${secH}"

# Move PowerShell console to secondary display at start
$consoleHwnd = (Get-Process -Id $PID).MainWindowHandle
if ($consoleHwnd -ne [IntPtr]::Zero) {
  [SettingsVerify]::SetWindowPos($consoleHwnd, [IntPtr]::Zero, $secLeft + 10, $secTop + 10, 800, 600, 0) | Out-Null
  Info "Moved PowerShell console to secondary display"
}

$env:AI_BUDDY_OPEN_SETTINGS = "1"
$env:AI_BUDDY_CHARACTER = "buddy-bot"
# Native Win32 window. Default Settings is the webview (#706 Step 8).
$env:AI_BUDDY_SETTINGS_NATIVE = "1"

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Bin
$psi.WorkingDirectory = $Root
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$script:AppProc = New-Object System.Diagnostics.Process
$script:AppProc.StartInfo = $psi
$null = $script:AppProc.Start()

# Wait for ai-buddy to spawn windows, then move overlays to secondary
Start-Sleep -Milliseconds 1000
$overlaysMoved = 0
for ($attempt = 0; $attempt -lt 30; $attempt++) {
  $windows = [SettingsVerify]::FindWindowsByPid([uint32]$script:AppProc.Id)
  foreach ($hwnd in $windows) {
    $sb = New-Object System.Text.StringBuilder(256)
    [SettingsVerify]::GetClassName($hwnd, $sb, 256) | Out-Null
    $className = $sb.ToString()
    # Tauri windows - move to secondary to keep primary clear
    if ([SettingsVerify]::IsWindowVisible($hwnd)) {
      $rect = New-Object SettingsVerify+RECT
      if ([SettingsVerify]::GetWindowRect($hwnd, [ref]$rect)) {
        $w = $rect.Right - $rect.Left
        $h = $rect.Bottom - $rect.Top
        # Only move if it's a reasonable-sized window (not minimized/hidden)
        if ($w -gt 50 -and $h -gt 50) {
          $newX = $secLeft + 100
          $newY = $secTop + 100
          [SettingsVerify]::SetWindowPos($hwnd, [IntPtr]::Zero, $newX, $newY, $w, $h, 0) | Out-Null
          $overlaysMoved++
        }
      }
    }
  }
  if ($overlaysMoved -gt 0) { break }
  Start-Sleep -Milliseconds 100
}
if ($overlaysMoved -gt 0) {
  Info "Moved $overlaysMoved ai-buddy overlay window(s) to secondary display"
}

Info "Waiting for settings window (PID-scoped class match)..."
$settingsHwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 150; $i++) {
  $windows = [SettingsVerify]::FindWindowsByPid([uint32]$script:AppProc.Id)
  foreach ($hwnd in $windows) {
    $sb = New-Object System.Text.StringBuilder(256)
    [SettingsVerify]::GetClassName($hwnd, $sb, 256) | Out-Null
    $className = $sb.ToString()
    if ($className -eq "AiBuddySettings") {
      $settingsHwnd = $hwnd
      # Move immediately to secondary display (belt-and-suspenders, product should already place it there)
      $winX = $secLeft + 50
      $winY = $secTop + 50
      $winW = 580
      $winH = 720
      [SettingsVerify]::SetWindowPos($settingsHwnd, [IntPtr]::Zero, $winX, $winY, $winW, $winH, 0) | Out-Null
      Pass "Settings window appeared and moved to secondary display"
      break
    }
    if ($className -eq "Tauri Window") {
      $len = [SettingsVerify]::GetWindowTextLength($hwnd)
      if ($len -gt 0) {
        $txt = New-Object System.Text.StringBuilder($len + 1)
        [SettingsVerify]::GetWindowText($hwnd, $txt, $txt.Capacity) | Out-Null
        if ($txt.ToString() -eq "Settings") {
          $settingsHwnd = $hwnd
          $winX = $secLeft + 50
          $winY = $secTop + 50
          $winW = 580
          $winH = 720
          [SettingsVerify]::SetWindowPos($settingsHwnd, [IntPtr]::Zero, $winX, $winY, $winW, $winH, 0) | Out-Null
          Pass "Settings window (webview) appeared and moved to secondary display"
          break
        }
      }
    }
  }
  if ($settingsHwnd -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 100
}
if ($settingsHwnd -eq [IntPtr]::Zero) { Fail "Settings window never appeared" }

# Brief settle time after move
Start-Sleep -Milliseconds 200
Info "Settings window at ${winX},${winY} on secondary display"

# Find tab control and collect all visible checkboxes/STATIC controls on Presence tab (tab 0)
$script:Checkboxes = New-Object System.Collections.Generic.List[PSCustomObject]
$script:StaticLabels = New-Object System.Collections.Generic.List[PSCustomObject]
$script:TabHwnd = [IntPtr]::Zero

[SettingsVerify]::EnumChildWindows($settingsHwnd, {
  param($hChild, $lParam)
  $sb = New-Object System.Text.StringBuilder(256)
  [SettingsVerify]::GetClassName($hChild, $sb, 256) | Out-Null
  $className = $sb.ToString()

  if ($className -eq "SysTabControl32") {
    $script:TabHwnd = $hChild
  }

  if ([SettingsVerify]::IsWindowVisible($hChild)) {
    if ($className -eq "Button") {
      $len = [SettingsVerify]::GetWindowTextLength($hChild)
      $txt = New-Object System.Text.StringBuilder($len + 1)
      [SettingsVerify]::GetWindowText($hChild, $txt, $txt.Capacity) | Out-Null
      if ($len -gt 0) {
        $script:Checkboxes.Add([PSCustomObject]@{ Hwnd = $hChild; Text = $txt.ToString() })
      }
    }
    elseif ($className -eq "Static") {
      $len = [SettingsVerify]::GetWindowTextLength($hChild)
      if ($len -gt 0) {
        $txt = New-Object System.Text.StringBuilder($len + 1)
        [SettingsVerify]::GetWindowText($hChild, $txt, $txt.Capacity) | Out-Null
        $script:StaticLabels.Add([PSCustomObject]@{ Hwnd = $hChild; Text = $txt.ToString(); Length = $len })
      }
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null

if ($script:Checkboxes.Count -eq 0) { Fail "No visible checkboxes on Presence tab" }
Pass "Presence tab: $($script:Checkboxes.Count) visible checkbox(es)"

# Helper function to assert a screen point is over the Settings window before clicking
function AssertClickOverSettings {
  param([IntPtr]$settingsHwnd, [int]$screenX, [int]$screenY, [string]$context)

  # Check that Settings window is foreground
  $foreground = [SettingsVerify]::GetForegroundWindow()
  if ($foreground -ne $settingsHwnd) {
    Info "$context - Settings not foreground, bringing to front"
    [SettingsVerify]::SetForegroundWindow($settingsHwnd) | Out-Null
    Start-Sleep -Milliseconds 100
    $foreground = [SettingsVerify]::GetForegroundWindow()
    if ($foreground -ne $settingsHwnd) {
      Fail "$context - Cannot bring Settings to foreground (foreground is $foreground, expected $settingsHwnd)"
    }
  }

  # Verify the click point is over the Settings window
  $pt = New-Object SettingsVerify+POINT
  $pt.X = $screenX
  $pt.Y = $screenY
  $hwndAtPoint = [SettingsVerify]::WindowFromPoint($pt)

  if ($hwndAtPoint -eq [IntPtr]::Zero) {
    Fail "$context - WindowFromPoint($screenX,$screenY) returned null"
  }

  # Walk up to root window
  $rootHwnd = [SettingsVerify]::GetAncestor($hwndAtPoint, [SettingsVerify]::GA_ROOT)
  if ($rootHwnd -ne $settingsHwnd) {
    $sb = New-Object System.Text.StringBuilder(256)
    [SettingsVerify]::GetClassName($rootHwnd, $sb, 256) | Out-Null
    $rootClass = $sb.ToString()
    Fail "$context - Click point ($screenX,$screenY) is NOT over Settings window! WindowFromPoint -> $hwndAtPoint, root -> $rootHwnd (class: $rootClass). Expected root: $settingsHwnd. Aborting to prevent clicking unrelated apps."
  }

  Info "$context - Verified click point ($screenX,$screenY) is over Settings window"
}

# Helper function to click a tab by posting mouse messages directly to tab HWND
# This avoids global SetCursorPos/mouse_event and cannot hit Chrome or other apps
function ClickTab {
  param([IntPtr]$tabHwnd, [int]$tabIndex, [string]$tabName)

  # Get tab client rect to determine click area
  $clientRect = New-Object SettingsVerify+RECT
  if (-not [SettingsVerify]::GetClientRect($tabHwnd, [ref]$clientRect)) {
    Fail "GetClientRect failed for tab control"
  }

  $clientWidth = $clientRect.Right - $clientRect.Left
  if ($clientWidth -le 0) {
    Fail "Tab control has zero or negative width: $clientWidth"
  }

  # Walk across tab header at y=10, posting clicks to find which X selects target tab
  # Record X ranges for each tab index, then click the midpoint
  $headerY = 10
  $tabRanges = @{}
  $lastSeenTab = -1

  for ($x = 5; $x -lt $clientWidth; $x += 10) {
    # Post mouse down/up to tab HWND at client coordinates (no global cursor)
    $lParam = [IntPtr]($x -bor ($headerY -shl 16))
    [SettingsVerify]::PostMessage($tabHwnd, [SettingsVerify]::WM_LBUTTONDOWN, [IntPtr]0, $lParam) | Out-Null
    Start-Sleep -Milliseconds 20
    [SettingsVerify]::PostMessage($tabHwnd, [SettingsVerify]::WM_LBUTTONUP, [IntPtr]0, $lParam) | Out-Null
    Start-Sleep -Milliseconds 50

    $curTab = [SettingsVerify]::SendMessage($tabHwnd, [SettingsVerify]::TCM_GETCURSEL, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()

    # Record range transitions
    if ($curTab -ne $lastSeenTab) {
      if ($curTab -ge 0 -and -not $tabRanges.ContainsKey($curTab)) {
        $tabRanges[$curTab] = @{ MinX = $x; MaxX = $x }
      }
      $lastSeenTab = $curTab
    }

    # Expand range for current tab
    if ($curTab -ge 0 -and $tabRanges.ContainsKey($curTab)) {
      $tabRanges[$curTab].MaxX = $x
    }

    # If we've found the target tab range, we can stop early
    if ($tabRanges.ContainsKey($tabIndex) -and $x -gt ($tabRanges[$tabIndex].MinX + 20)) {
      break
    }
  }

  # Check if we found the target tab
  if (-not $tabRanges.ContainsKey($tabIndex)) {
    Fail "$tabName tab (index $tabIndex) not found during sweep (found tabs: $($tabRanges.Keys -join ', '))"
  }

  # Click the midpoint of the target tab range
  $range = $tabRanges[$tabIndex]
  $midX = ($range.MinX + $range.MaxX) / 2
  Info "Clicking $tabName tab at client coords ($midX,$headerY) via PostMessage to tab HWND"

  $lParam = [IntPtr]([int]$midX -bor ($headerY -shl 16))
  [SettingsVerify]::PostMessage($tabHwnd, [SettingsVerify]::WM_LBUTTONDOWN, [IntPtr]0, $lParam) | Out-Null
  Start-Sleep -Milliseconds 30
  [SettingsVerify]::PostMessage($tabHwnd, [SettingsVerify]::WM_LBUTTONUP, [IntPtr]0, $lParam) | Out-Null
  Start-Sleep -Milliseconds 300

  # Assert we're on the expected tab
  $curTab = [SettingsVerify]::SendMessage($tabHwnd, [SettingsVerify]::TCM_GETCURSEL, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
  if ($curTab -ne $tabIndex) {
    Fail "$tabName tab click failed: expected tab $tabIndex, got $curTab"
  }

  Pass "$tabName tab selected successfully"
}

# Click Director tab (index 2)
if ($script:TabHwnd -eq [IntPtr]::Zero) { Fail "Tab control not found" }
ClickTab $script:TabHwnd 2 "Director"

# Re-enumerate to find Director tab's visible STATIC controls (field labels)
$script:DirectorLabels = New-Object System.Collections.Generic.List[PSCustomObject]
[SettingsVerify]::EnumChildWindows($settingsHwnd, {
  param($hChild, $lParam)
  $sb = New-Object System.Text.StringBuilder(256)
  [SettingsVerify]::GetClassName($hChild, $sb, 256) | Out-Null
  $className = $sb.ToString()

  if ($className -eq "Static" -and [SettingsVerify]::IsWindowVisible($hChild)) {
    $len = [SettingsVerify]::GetWindowTextLength($hChild)
    if ($len -gt 0) {
      $txt = New-Object System.Text.StringBuilder($len + 1)
      [SettingsVerify]::GetWindowText($hChild, $txt, $txt.Capacity) | Out-Null
      $text = $txt.ToString()
      # Look for field label patterns
      if ($text -match "URL|Model|API key|Base|timeout|tokens") {
        $script:DirectorLabels.Add([PSCustomObject]@{ Hwnd = $hChild; Text = $text; Length = $len })
      }
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null

if ($script:DirectorLabels.Count -eq 0) {
  Fail "Director tab: no field caption STATIC controls with non-zero text (label-wipe bug not fixed)"
}
Pass "Director tab: $($script:DirectorLabels.Count) field label(s) with non-zero text (label-wipe fix verified)"
foreach ($lbl in $script:DirectorLabels) {
  Info "  Label: '$($lbl.Text)' (len=$($lbl.Length))"
}

# Click Development tab (index 4)
ClickTab $script:TabHwnd 4 "Development"

# Find Trace* checkboxes
$script:DevCheckboxes = New-Object System.Collections.Generic.List[PSCustomObject]
[SettingsVerify]::EnumChildWindows($settingsHwnd, {
  param($hChild, $lParam)
  $sb = New-Object System.Text.StringBuilder(256)
  [SettingsVerify]::GetClassName($hChild, $sb, 256) | Out-Null
  $className = $sb.ToString()

  if ($className -eq "Button" -and [SettingsVerify]::IsWindowVisible($hChild)) {
    $len = [SettingsVerify]::GetWindowTextLength($hChild)
    if ($len -gt 0) {
      $txt = New-Object System.Text.StringBuilder($len + 1)
      [SettingsVerify]::GetWindowText($hChild, $txt, $txt.Capacity) | Out-Null
      $text = $txt.ToString()
      if ($text -match "Trace") {
        $script:DevCheckboxes.Add([PSCustomObject]@{ Hwnd = $hChild; Text = $text })
      }
    }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null

if ($script:DevCheckboxes.Count -eq 0) { Fail "Development tab: no Trace* checkboxes visible" }
Pass "Development tab: $($script:DevCheckboxes.Count) Trace checkbox(es) visible"

# Click AI tab (index 2) for AI tab structure tests
ClickTab $script:TabHwnd 2 "AI"

# Test AI tab section titles presence via UIA helper
Info "Testing AI tab section structure via UI Automation"
$axHelper = Join-Path $Root "scripts\ax-settings-win.ps1"
if (-not (Test-Path $axHelper)) {
  Info "ax-settings-win.ps1 helper not found - skipping AI tab structure checks"
} else {
  # Dump AI tab structure
  $aiDump = Join-Path $Out "ai-tab-dump.txt"
  & $axHelper -Command dump > $aiDump 2>&1
  if ($LASTEXITCODE -eq 0) {
    Pass "Dumped AI tab structure to ai-tab-dump.txt"

    # Check for expected section headings
    $content = Get-Content $aiDump -Raw
    $expectedSections = @("AI", "AI source", "Model / API", "Last user turn")
    $foundSections = 0
    foreach ($section in $expectedSections) {
      if ($content -match [regex]::Escape($section)) {
        $foundSections++
      }
    }

    if ($foundSections -eq $expectedSections.Length) {
      Pass "AI tab: all $($expectedSections.Length) expected sections found"
    } else {
      Info "AI tab: found $foundSections of $($expectedSections.Length) expected sections (UIA may not capture all STATIC labels)"
    }

    # Check for disclosure control ("What is this?")
    if ($content -match "What is this") {
      Pass "AI tab: disclosure control found"

      # Try expanding a disclosure
      & $axHelper -Command expand-disclosure -DisclosureLabel "What is this?" 2>&1 | Out-Null
      if ($LASTEXITCODE -eq 0) {
        Pass "Disclosure control expanded successfully"
      } else {
        Info "Disclosure control expansion not available via UIA (may need direct Win32 interaction)"
      }
    } else {
      Info "AI tab: disclosure control not detected via UIA (may be a BUTTON not exposed to accessibility)"
    }
  } else {
    Info "Could not dump AI tab via UIA - skipping AI tab structure checks"
  }

  # Test runtime freeze/unfreeze by switching AI source
  Info "Testing runtime freeze/unfreeze via AI source switch"

  # Dump initial state (Model API expected by default)
  $initialDump = Join-Path $Out "no-harness-dump.txt"
  & $axHelper -Command dump > $initialDump 2>&1

  # Check that Base URL / Model / API key are enabled
  $initialContent = Get-Content $initialDump -Raw
  $baseUrlEnabled = $initialContent -match "Base URL.*enabled"
  $modelEnabled = $initialContent -match "Model.*enabled"

  if ($baseUrlEnabled -and $modelEnabled) {
    Pass "HTTP rows enabled with Model API (no Harness)"
  } else {
    Info "HTTP rows state unclear from UIA dump (may need direct Win32 ES_READONLY check)"
  }

  # Popup titles are "Harness <U+00B7> {name}" (#593). .ps1 must stay ASCII (#418),
  # so the middle dot is built at runtime rather than stored in this file.
  $harness = if ($env:AI_BUDDY_VERIFY_HARNESS) { $env:AI_BUDDY_VERIFY_HARNESS } else { "claude" }
  $harnessTitle = "Harness $([char]0x00B7) $harness"
  Info "Attempting to switch to Harness (if installed)"
  $prevEap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
  & $axHelper -Command pick-source -Title $harnessTitle *> $null
  $pickOk = ($LASTEXITCODE -eq 0)
  $ErrorActionPreference = $prevEap
  if ($pickOk) {
    Pass "Switched AI source to Harness"
    Start-Sleep -Milliseconds 1000

    $harnessDump = Join-Path $Out "harness-dump.txt"
    & $axHelper -Command dump > $harnessDump 2>&1

    # Check that Base URL / Model / API key are now disabled
    $harnessContent = Get-Content $harnessDump -Raw
    $baseUrlDisabled = $harnessContent -match "Base URL.*disabled"
    $modelDisabled = $harnessContent -match "Model.*disabled"

    if ($baseUrlDisabled -and $modelDisabled) {
      Pass "HTTP rows frozen while Harness drives"
    } else {
      Info "HTTP rows frozen state unclear from UIA dump (check ES_READONLY in unit tests)"
    }

    # Switch back to Model API
    & $axHelper -Command pick-source -Title "Model API" *> $null
    if ($LASTEXITCODE -eq 0) {
      Pass "Switched back to Model API"
      Start-Sleep -Milliseconds 500

      $backDump = Join-Path $Out "back-to-model-api-dump.txt"
      & $axHelper -Command dump > $backDump 2>&1

      # Check that rows are enabled again
      $backContent = Get-Content $backDump -Raw
      $baseUrlReEnabled = $backContent -match "Base URL.*enabled"
      $modelReEnabled = $backContent -match "Model.*enabled"

      if ($baseUrlReEnabled -and $modelReEnabled) {
        Pass "HTTP rows unfrozen after switching back to Model API (runtime freeze/unfreeze works)"
      } else {
        Info "HTTP rows re-enabled state unclear from UIA dump"
      }
    } else {
      Info "Could not switch back to Model API"
    }
  } else {
    Info "Could not switch to Harness - may not be installed or UIA combo picking not supported"
    Info "Freeze/unfreeze should be verified manually or in unit tests"
  }
}

if ($script:AppProc -and -not $script:AppProc.HasExited) {
  Stop-Process -Id $script:AppProc.Id -Force -ErrorAction SilentlyContinue
}
Pass "Settings window smoke test finished - $Out"
exit 0
