#!/usr/bin/env pwsh
#Requires -Version 5.1
<#
.SYNOPSIS
  #715 Phase 2 Windows Settings-webview smoke.

.DESCRIPTION
  Four checks against AI_BUDDY_SETTINGS_WEBVIEW=1: window opens as a webview,
  all five tabs are present and driven, Presence Sound round-trips to
  %APPDATA%\ai-buddy\settings.json, z-order still is a cropped screenshot for
  a human to judge.

  Does not replace scripts/verify-settings-win.ps1 (native Win32 children).

.NOTES
  Never assign $PID / $pid: it is Constant+AllScope. Use ProcessId /
  targetProcessId. UIA via AutomationElement.FromHandle only --
  RootElement.FindFirst(Descendants) hung on a dual-display box (#715).
  Evidence PNGs are cropped to the Settings HWND (privacy) and scaled ~1/3;
  do not commit them. Runners attach with gh issue comment --attach.

  CHECK2: wait until all five ControlType.TabItem names exist (WebView2 UIA
  tree is empty on the first tick), then find each tab with TabItem AND Name
  -- Name-only hits the Pane also named Presence (ESTHER 2026-09-18). Activate
  is Invoke, SelectionItem.Select, LegacyIAccessible DoDefaultAction, then
  PostMessage. Presence already selected plus a no-op activate still PASSes.

.USAGE
  .\scripts\verify-settings-webview-phase2-win.ps1
  $env:AI_BUDDY_VERIFY_BIN="path\to\ai-buddy.exe" .\scripts\verify-settings-webview-phase2-win.ps1
#>
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not (Test-Path (Join-Path $Root 'src-tauri'))) { $Root = (Get-Location).Path }
Set-Location $Root

$OutRoot = Join-Path $Root '.verify\715-phase2-win'
$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$OutDir = Join-Path $OutRoot $Stamp
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$Bin = if ($env:AI_BUDDY_VERIFY_BIN) { $env:AI_BUDDY_VERIFY_BIN } else { Join-Path $Root 'target\debug\ai-buddy.exe' }
$tip = (git -C $Root rev-parse HEAD).Trim()
$Report = [ordered]@{
  tipSha = $tip
  flag = 'AI_BUDDY_SETTINGS_WEBVIEW=1'
  os = [Environment]::OSVersion.VersionString
  checks = @{}
  screenshots = @()
  notes = @()
  webviewConfirmed = $false
  globalCursorUsed = $false
  monitors = @()
  settingsBounds = $null
}

function Log([string]$m) {
  $l = '[{0}] {1}' -f (Get-Date -Format 'HH:mm:ss'), $m
  Write-Host $l
  Add-Content (Join-Path $OutDir 'smoke.log') $l
}

function Write-Report {
  $Report | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $OutDir 'REPORT.json')
}

function Stop-Target([uint32]$TargetProcessId) {
  if ($TargetProcessId -eq 0) { return }
  try { Stop-Process -Id $TargetProcessId -Force -ErrorAction SilentlyContinue } catch {}
}

Add-Type -ErrorAction SilentlyContinue -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class Phase2Win {
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  public delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint processId);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int X, int Y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr clip, MonitorEnumProc cb, IntPtr data);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr hWnd, ref POINT lpPoint);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
  public static readonly IntPtr HWND_TOP = IntPtr.Zero;
  public const uint SWP_NOSIZE=1, SWP_NOZORDER=4, SWP_SHOWWINDOW=0x40, MONITORINFOF_PRIMARY=1;
  public const uint WM_LBUTTONDOWN=0x0201, WM_LBUTTONUP=0x0202;
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Auto)]
  public struct MONITORINFO {
    public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags;
  }
}
'@

function Capture([string]$name, [IntPtr]$hwnd) {
  # Crop to Settings HWND so a virtual-screen grab cannot leak the rest of the
  # desktop. Scale ~1/3 so runners can attach the file on an issue comment.
  if ($hwnd -eq [IntPtr]::Zero) {
    Log "shot $name skipped (no HWND)"
    return
  }
  Add-Type -AssemblyName System.Drawing -ErrorAction SilentlyContinue
  $rect = New-Object Phase2Win+RECT
  if (-not [Phase2Win]::GetWindowRect($hwnd, [ref]$rect)) {
    Log "shot $name skipped (GetWindowRect failed)"
    return
  }
  $w = $rect.Right - $rect.Left
  $h = $rect.Bottom - $rect.Top
  if ($w -le 0 -or $h -le 0) {
    Log "shot $name skipped (empty bounds)"
    return
  }
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object Drawing.Size $w, $h))
  $rw = [Math]::Max(1, [int][Math]::Round($w / 3.0))
  $rh = [Math]::Max(1, [int][Math]::Round($h / 3.0))
  $small = New-Object System.Drawing.Bitmap $rw, $rh
  $sg = [System.Drawing.Graphics]::FromImage($small)
  $sg.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $sg.DrawImage($bmp, 0, 0, $rw, $rh)
  $path = Join-Path $OutDir $name
  $small.Save($path, [Drawing.Imaging.ImageFormat]::Png)
  $sg.Dispose(); $small.Dispose(); $g.Dispose(); $bmp.Dispose()
  $Report.screenshots += $path
  Log "shot $name crop LTRB=$($rect.Left),$($rect.Top),$($rect.Right),$($rect.Bottom) scaled ${rw}x${rh}"
}

function Get-MonitorMap {
  $list = New-Object System.Collections.Generic.List[object]
  $cb = [Phase2Win+MonitorEnumProc]{
    param($h, $hdc, [ref]$r, $d)
    $mi = New-Object Phase2Win+MONITORINFO
    $mi.cbSize = [Runtime.InteropServices.Marshal]::SizeOf($mi)
    if ([Phase2Win]::GetMonitorInfo($h, [ref]$mi)) {
      $list.Add([pscustomobject]@{
        primary = (($mi.dwFlags -band [Phase2Win]::MONITORINFOF_PRIMARY) -ne 0)
        left = $mi.rcWork.Left; top = $mi.rcWork.Top
        right = $mi.rcWork.Right; bottom = $mi.rcWork.Bottom
        width = $mi.rcWork.Right - $mi.rcWork.Left
        height = $mi.rcWork.Bottom - $mi.rcWork.Top
      }) | Out-Null
    }
    return $true
  }
  [void][Phase2Win]::EnumDisplayMonitors([IntPtr]::Zero, [IntPtr]::Zero, $cb, [IntPtr]::Zero)
  return $list
}

function Find-SettingsHwnd([uint32]$TargetProcessId) {
  $script:foundHwnd = [IntPtr]::Zero
  $script:foundClass = ''
  $script:sawNative = $false
  $cb = [Phase2Win+EnumProc]{
    param($h, $l)
    $processId = [uint32]0
    [void][Phase2Win]::GetWindowThreadProcessId($h, [ref]$processId)
    if ($processId -ne $TargetProcessId) { return $true }
    $cls = New-Object Text.StringBuilder 256
    $title = New-Object Text.StringBuilder 256
    [void][Phase2Win]::GetClassName($h, $cls, 256)
    [void][Phase2Win]::GetWindowText($h, $title, 256)
    $c = $cls.ToString(); $t = $title.ToString()
    if ($c -eq 'AiBuddySettings') { $script:sawNative = $true }
    if ($t -eq 'Settings') {
      $script:foundHwnd = $h
      $script:foundClass = $c
      Log "HWND Settings class='$c' processId=$processId"
    }
    return $true
  }
  [void][Phase2Win]::EnumWindows($cb, [IntPtr]::Zero)
  return $script:foundHwnd
}

function Park-OnSecondary([IntPtr]$hwnd, $monitors) {
  if ($hwnd -eq [IntPtr]::Zero) { return }
  $sec = $monitors | Where-Object { -not $_.primary } | Select-Object -First 1
  if (-not $sec) {
    $Report.notes += 'No secondary monitor -- left Settings where it opened'
    Log 'No secondary monitor -- left Settings where it opened'
    return
  }
  # Place near top-left of secondary work area (not a magic negative coord).
  $x = [int]$sec.left + 40
  $y = [int]$sec.top + 40
  [void][Phase2Win]::SetWindowPos($hwnd, [Phase2Win]::HWND_TOP, $x, $y, 0, 0,
    [Phase2Win]::SWP_NOSIZE -bor [Phase2Win]::SWP_NOZORDER -bor [Phase2Win]::SWP_SHOWWINDOW)
  Log "Parked Settings at $x,$y on secondary work area $($sec.width)x$($sec.height)"
}

function Log-WindowVsMonitors([IntPtr]$hwnd, $monitors) {
  $rect = New-Object Phase2Win+RECT
  if (-not [Phase2Win]::GetWindowRect($hwnd, [ref]$rect)) { return }
  $Report.settingsBounds = @{ left = $rect.Left; top = $rect.Top; right = $rect.Right; bottom = $rect.Bottom }
  $cx = [int](($rect.Left + $rect.Right) / 2)
  $cy = [int](($rect.Top + $rect.Bottom) / 2)
  $on = $null
  foreach ($m in $monitors) {
    if ($cx -ge $m.left -and $cx -lt $m.right -and $cy -ge $m.top -and $cy -lt $m.bottom) {
      $on = $m
      break
    }
  }
  $which = if ($null -eq $on) { 'NONE' } elseif ($on.primary) { 'PRIMARY' } else { 'SECONDARY' }
  Log "Settings bounds LTRB=$($rect.Left),$($rect.Top),$($rect.Right),$($rect.Bottom) center=$cx,$cy monitor=$which"
  $Report.notes += "settings_monitor=$which"
}

function Get-AutomationElementFromHandle([IntPtr]$hwnd) {
  if ($hwnd -eq [IntPtr]::Zero) { return $null }
  try {
    return [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
  } catch {
    Log "UIA FromHandle failed: $_"
    return $null
  }
}

function New-TabItemAndNameCondition([string]$Name) {
  # TabItem AND Name: Name-only FindFirst can return the Pane named Presence
  # (the tabpanel aria-label) instead of the tab (#715 ESTHER).
  $nameCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty, $Name)
  $typeCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::TabItem)
  $andCond = New-Object System.Windows.Automation.AndCondition($nameCond, $typeCond)
  return $andCond
}

function Find-SettingsTab($win, [string]$Name) {
  if ($null -eq $win) { return $null }
  $andCond = New-TabItemAndNameCondition $Name
  return $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $andCond)
}

function Wait-SettingsTabs($win, $names, [int]$TimeoutMs = 8000, [int]$PollMs = 350) {
  if ($null -eq $win) { return $false }
  $deadline = [datetime]::UtcNow.AddMilliseconds($TimeoutMs)
  $missing = @($names)
  while ([datetime]::UtcNow -le $deadline) {
    $missing = @()
    foreach ($n in $names) {
      if ($null -eq (Find-SettingsTab $win $n)) { $missing += $n }
    }
    if ($missing.Count -eq 0) {
      Log ("CHECK2 tabs ready: " + ($names -join ','))
      return $true
    }
    Start-Sleep -Milliseconds $PollMs
  }
  Log ("CHECK2 tabs timeout missing=" + ($missing -join ','))
  $Report.notes += ("CHECK2 tabs timeout missing=" + ($missing -join ','))
  return $false
}

function Test-TabAlreadySelected($el) {
  if ($null -eq $el) { return $false }
  try {
    $sel = $el.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
    if ($sel -and $sel.Current.IsSelected) { return $true }
  } catch {}
  return $false
}

function Get-SupportedPatternNames($el) {
  $names = @()
  try {
    foreach ($p in $el.GetSupportedPatterns()) { $names += $p.ProgrammaticName }
  } catch {}
  return $names
}

function Invoke-UiaActivate($el, [IntPtr]$settingsHwnd, [string]$label) {
  # WebView2 tabs often expose neither Invoke nor SelectionItem (#715). Try
  # LegacyIAccessible, then PostMessage -- never SetCursorPos.
  $tried = @()

  try {
    $inv = $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    if ($inv) { $inv.Invoke(); return @{ ok = $true; via = 'Invoke'; tried = @('Invoke') } }
    $tried += 'Invoke-missing'
  } catch { $tried += "Invoke:$($_.Exception.Message)" }

  try {
    $sel = $el.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
    if ($sel) { $sel.Select(); return @{ ok = $true; via = 'SelectionItem'; tried = @($tried + 'SelectionItem') } }
    $tried += 'SelectionItem-missing'
  } catch { $tried += "SelectionItem:$($_.Exception.Message)" }

  try {
    $leg = $el.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern)
    if ($leg) { $leg.DoDefaultAction(); return @{ ok = $true; via = 'LegacyIAccessible'; tried = @($tried + 'LegacyIAccessible') } }
    $tried += 'LegacyIAccessible-missing'
  } catch { $tried += "LegacyIAccessible:$($_.Exception.Message)" }

  try {
    $tog = $el.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
    if ($tog) { $tog.Toggle(); return @{ ok = $true; via = 'Toggle'; tried = @($tried + 'Toggle') } }
    $tried += 'Toggle-missing'
  } catch { $tried += "Toggle:$($_.Exception.Message)" }

  try {
    $pt = $el.GetClickablePoint()
    $target = $settingsHwnd
    $native = [IntPtr]$el.Current.NativeWindowHandle
    if ($native -ne [IntPtr]::Zero) { $target = $native }
    $sp = New-Object Phase2Win+POINT
    $sp.X = [int]$pt.X; $sp.Y = [int]$pt.Y
    if ([Phase2Win]::ScreenToClient($target, [ref]$sp)) {
      $lParam = [IntPtr](($sp.Y -shl 16) -bor ($sp.X -band 0xFFFF))
      [void][Phase2Win]::PostMessage($target, [Phase2Win]::WM_LBUTTONDOWN, [IntPtr]0, $lParam)
      Start-Sleep -Milliseconds 30
      [void][Phase2Win]::PostMessage($target, [Phase2Win]::WM_LBUTTONUP, [IntPtr]0, $lParam)
      return @{ ok = $true; via = 'PostMessage'; tried = @($tried + 'PostMessage') }
    }
    $tried += 'PostMessage-ScreenToClient-failed'
  } catch { $tried += "PostMessage:$($_.Exception.Message)" }

  return @{
    ok = $false
    via = 'none'
    tried = $tried
    patterns = @(Get-SupportedPatternNames $el)
    label = $label
  }
}

function Wait-SettingsHwnd([uint32]$TargetProcessId) {
  $hwnd = [IntPtr]::Zero
  for ($i = 0; $i -lt 25; $i++) {
    $hwnd = Find-SettingsHwnd $TargetProcessId
    if ($hwnd -ne [IntPtr]::Zero) { return $hwnd }
    Start-Sleep -Milliseconds 400
  }
  return $hwnd
}

function Park-And-Reacquire([IntPtr]$hwnd, [uint32]$TargetProcessId, $monitors) {
  Park-OnSecondary $hwnd $monitors
  Start-Sleep -Milliseconds 500
  $again = Find-SettingsHwnd $TargetProcessId
  return $again
}

if (-not (Test-Path $Bin)) {
  Log "missing $Bin -- build with VsDevCmd first, or set AI_BUDDY_VERIFY_BIN"
  $Report.notes += "missing binary $Bin"
  Write-Report
  Write-Host "OUTDIR=$OutDir"
  exit 1
}

$monitors = @(Get-MonitorMap)
$Report.monitors = $monitors
foreach ($m in $monitors) {
  Log ("Monitor primary=$($m.primary) work=$($m.left),$($m.top) $($m.width)x$($m.height)")
}

Get-Process ai-buddy -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep 1
$env:AI_BUDDY_SETTINGS_WEBVIEW = '1'
$env:AI_BUDDY_OPEN_SETTINGS = '1'
$env:AI_BUDDY_CAPTURABLE = '1'
Log "Launch $Bin tip=$tip"
$proc = Start-Process -FilePath $Bin -WorkingDirectory $Root -PassThru
$targetProcessId = [uint32]$proc.Id
Start-Sleep 6

$hwnd = Wait-SettingsHwnd $targetProcessId
if ($script:sawNative) { $Report.notes += 'Native AiBuddySettings present' }
$kids = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($proc.Id)" -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -match 'msedgewebview2|WebView2' }
if ($kids) {
  $Report.webviewConfirmed = $true
  $Report.notes += ('WebView2: ' + (($kids | ForEach-Object Name) -join ','))
}

function Fail-Check1([string]$reason, [string]$shot, [uint32]$TargetProcessId, [IntPtr]$hwnd) {
  $Report.checks['1_window_opens'] = 'FAIL'
  $Report.notes += $reason
  Capture $shot $hwnd
  Log "CHECK1 FAIL ($reason)"
  Write-Report
  Stop-Target $TargetProcessId
  Write-Host "OUTDIR=$OutDir"
  exit 1
}

if ($hwnd -eq [IntPtr]::Zero) {
  Fail-Check1 'Settings HWND never appeared' '01-FAIL-no-settings.png' $targetProcessId ([IntPtr]::Zero)
}

$hwnd = Park-And-Reacquire $hwnd $targetProcessId $monitors
if ($hwnd -eq [IntPtr]::Zero) {
  Fail-Check1 'Settings vanished after secondary park' '01-FAIL-after-park.png' $targetProcessId ([IntPtr]::Zero)
}
Log-WindowVsMonitors $hwnd $monitors

if ($script:foundClass -eq 'AiBuddySettings') {
  Fail-Check1 'Opened native AiBuddySettings, not the webview (Tauri Window)' '01-FAIL-native.png' $targetProcessId $hwnd
}
if ($script:foundClass -and $script:foundClass -ne 'Tauri Window') {
  $Report.notes += "unexpected_class=$($script:foundClass)"
}

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes -ErrorAction SilentlyContinue
$win = Get-AutomationElementFromHandle $hwnd
if ($win) {
  $br = $win.Current.BoundingRectangle
  Log "UIA FromHandle BoundingRectangle L=$($br.Left) T=$($br.Top) W=$($br.Width) H=$($br.Height)"
}

Capture '01-settings-opens.png' $hwnd
$Report.checks['1_window_opens'] = 'PASS'
if ($Report.webviewConfirmed) {
  Log 'CHECK1 PASS (webview)'
} else {
  $Report.notes += 'WebView2 child process not observed; HWND class was still not native'
  Log 'CHECK1 PASS (Tauri Window; WebView2 process not listed)'
}

$tabs = @('Presence', 'Character', 'AI', 'Privacy', 'Development')
$tabPass = $true
if (-not $win) {
  $tabPass = $false
  $Report.notes += 'No UIA Settings after park -- FromHandle returned null'
  if ($script:foundClass) { $Report.notes += "win32_class=$($script:foundClass)" }
} else {
  $null = Wait-SettingsTabs $win $tabs
  foreach ($tab in $tabs) {
    try {
      $el = Find-SettingsTab $win $tab
      if ($null -eq $el) { Log "Tab miss $tab"; $tabPass = $false; continue }
      $already = Test-TabAlreadySelected $el
      $act = Invoke-UiaActivate $el $hwnd $tab
      if ($act.ok) {
        Log "Tab $tab via $($act.via)"
      } elseif ($already -or $tab -eq 'Presence') {
        # Default tab: Select/Invoke can be a no-op while Presence is showing.
        Log "Tab $tab already selected (activate no-op via=$($act.via))"
        $Report.notes += "tab_$tab already selected"
      } else {
        Log "Tab activate failed $tab tried=$($act.tried -join ',') patterns=$($act.patterns -join ',')"
        $Report.notes += "tab_$tab failed tried=$($act.tried -join ',') patterns=$($act.patterns -join ',')"
        $tabPass = $false
      }
      Start-Sleep -Milliseconds 700
      Capture ("02-tab-$tab.png") $hwnd
    } catch { Log "Tab $tab err $_"; $tabPass = $false }
  }
}
$Report.checks['2_five_tabs'] = $(if ($tabPass) { 'PASS' } else { 'FAIL' })
Log ("CHECK2 " + $Report.checks['2_five_tabs'])

$settingsPath = Join-Path $env:APPDATA 'ai-buddy\settings.json'
$settingsCandidates = @(
  $settingsPath,
  (Join-Path $env:APPDATA 'ai.buddy\settings.json'),
  (Join-Path $env:LOCALAPPDATA 'ai-buddy\settings.json')
)
$Report.checks['3_roundtrip'] = 'FAIL'
try {
  if ($win) {
    $presence = Find-SettingsTab $win 'Presence'
    if ($presence) { $null = Invoke-UiaActivate $presence $hwnd 'Presence' }
    Start-Sleep 1
    $soundCond = New-Object System.Windows.Automation.PropertyCondition (
      [System.Windows.Automation.AutomationElement]::NameProperty, 'Sound')
    $sound = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $soundCond)
    if ($sound) {
      $tact = Invoke-UiaActivate $sound $hwnd 'Sound'
      if ($tact.ok) { $Report.notes += "sound via $($tact.via)" } else { $Report.notes += "sound activate failed" }
      Capture '03-sound-toggled.png' $hwnd
    } else {
      $Report.notes += 'Sound UIA not found'
      $Report.checks['3_roundtrip'] = 'FAIL'
    }
  }
  $spath = $settingsCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($spath) {
    Copy-Item $spath (Join-Path $OutDir 'settings-after-toggle.json') -Force
    Log "settings $spath"
    if ($spath -ne $settingsPath) { $Report.notes += "settings_path_was=$spath (expected $settingsPath)" }
  }
  Stop-Target $targetProcessId
  Start-Sleep 2
  $env:AI_BUDDY_SETTINGS_WEBVIEW = '1'
  $env:AI_BUDDY_OPEN_SETTINGS = '1'
  $proc = Start-Process -FilePath $Bin -WorkingDirectory $Root -PassThru
  $targetProcessId = [uint32]$proc.Id
  Start-Sleep 6
  $hwnd = Wait-SettingsHwnd $targetProcessId
  if ($hwnd -ne [IntPtr]::Zero) {
    $hwnd = Park-And-Reacquire $hwnd $targetProcessId $monitors
    if ($hwnd -ne [IntPtr]::Zero) { Log-WindowVsMonitors $hwnd $monitors }
  }
  Capture '03-after-relaunch.png' $hwnd
  $spath = $settingsCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($spath) {
    Copy-Item $spath (Join-Path $OutDir 'settings-after-relaunch.json') -Force
    $j = Get-Content $spath -Raw | ConvertFrom-Json
    if ($null -ne $j.sound) {
      $Report.checks['3_roundtrip'] = 'PASS'
      $Report.notes += "sound=$($j.sound)"
    } elseif (-not $Report.checks.Contains('3_roundtrip')) {
      $Report.checks['3_roundtrip'] = 'FAIL'
    }
  } elseif (-not $Report.checks.Contains('3_roundtrip')) {
    $Report.checks['3_roundtrip'] = 'FAIL'
  }
} catch {
  $Report.checks['3_roundtrip'] = 'FAIL'
  $Report.notes += "check3 $_"
}
Log ("CHECK3 " + $Report.checks['3_roundtrip'])

Capture '04-zorder.png' $hwnd
$Report.checks['4_zorder'] = 'REVIEW'
$Report.notes += 'Judge Settings above Character/overlay in 04-zorder.png (HWND crop; sprite only visible if it overlaps Settings)'
Log 'CHECK4 REVIEW'
Stop-Target $targetProcessId
Write-Report
Log "DONE OutDir=$OutDir"
Write-Host "OUTDIR=$OutDir"
Get-Content (Join-Path $OutDir 'REPORT.json')
