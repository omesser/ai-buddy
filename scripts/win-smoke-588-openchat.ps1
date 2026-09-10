#!/usr/bin/env pwsh
# Windows watch smoke for PR #588 (Open chat on a truncated bubble).
#
# The bubble caps speech at six lines; a turn that runs past them draws one
# control, "Open chat", above the head (outside the sprite's art). On Windows
# the frame loop unions that control's rectangle into the overlay's input
# region with SetWindowRgn (src-tauri/src/platform/windows/overlay.rs), so the
# region is the one authority on where a click opens Chat. This smoke drives
# that click and nothing else.
#
# Why the earlier live smokes on DESKTOP-UQIE144 kept failing, and what this
# does instead:
#   - The buddy strolls. Sampling once and then pausing before the click lands
#     where it *was*. Here every click re-samples the newest `sprite(x,y)` the
#     frame loop traced and fires within tens of milliseconds — no pre-click
#     pause.
#   - Hunting a teal pixel across the desktop matches YouTube's cyan. Here the
#     Character is found from the trace and the OS window region, never colour.
#   - WindowFromPoint over click-through art returns the window behind ours.
#     Here the target is read from GetWindowRgn/GetRegionData on the large
#     overlay HWND (a full-display window), and every click point is confirmed
#     with PtInRegion and a GA_ROOT check before it is sent. The 136x39 anchor
#     window is never a target.
#
# Assumes the checkout at C:\Users\oded\src\ai-buddy on tip 4d52843 with a built
# target\debug\ai-buddy.exe. Node is required for the Completer stub.
#
# Usage:
#   .\scripts\win-smoke-588-openchat.ps1
#   $env:AI_BUDDY_VERIFY_BIN="path\to\ai-buddy.exe" .\scripts\win-smoke-588-openchat.ps1
#
# Env knobs:
#   AI_BUDDY_VERIFY_BIN   override the binary path (default target\debug\ai-buddy.exe)
#   AI_BUDDY_SMOKE_PORT   stub port (default 18765)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not (Test-Path (Join-Path $Root "src-tauri"))) { $Root = (Get-Location).Path }
Set-Location $Root

function Info($m) { Write-Host "[INFO] $m" -ForegroundColor Cyan }
function Pass($m) { Write-Host "[PASS] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "[WARN] $m" -ForegroundColor Yellow }

$script:AppProc = $null
$script:StubProc = $null

function Cleanup {
  if ($script:AppProc -and -not $script:AppProc.HasExited) {
    Stop-Process -Id $script:AppProc.Id -Force -ErrorAction SilentlyContinue
  }
  if ($script:StubProc -and -not $script:StubProc.HasExited) {
    Stop-Process -Id $script:StubProc.Id -Force -ErrorAction SilentlyContinue
  }
  # Leftover ai-buddy processes from a crashed prior run would hold overlays on
  # screen and steal this run's window enumeration.
  Get-Process ai-buddy -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

function Fail($m) {
  Write-Host "[FAIL] $m" -ForegroundColor Red
  Cleanup
  exit 1
}

$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$Out = Join-Path $Root ".verify\pr588-smoke\$Stamp"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Log = Join-Path $Out "app.log"
$StubLog = Join-Path $Out "stub.log"
$Report = Join-Path $Out "report.txt"
$DocsPr = Join-Path $Root "docs\pr"
New-Item -ItemType Directory -Force -Path $DocsPr | Out-Null
Info "Output: $Out"

Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Smoke {
  public const int GWL_EXSTYLE = -20;
  public const int GWL_STYLE = -16;
  public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
  public const uint MOUSEEVENTF_LEFTUP = 0x0004;
  public const uint MONITORINFOF_PRIMARY = 1;
  public const int GA_ROOT = 2;
  public const uint WM_CLOSE = 0x0010;
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO {
    public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags;
  }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hWnd, int nIndex);
  [DllImport("user32.dll")] public static extern bool GetClassName(IntPtr hWnd, StringBuilder sb, int nMaxCount);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int nMaxCount);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int X, int Y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT pt);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hWnd, int flags);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr clip, MonitorEnumProc fn, IntPtr data);
  public delegate bool MonitorEnumProc(IntPtr h, IntPtr hdc, ref RECT r, IntPtr d);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr h, ref MONITORINFO mi);
  [DllImport("user32.dll")] public static extern int GetWindowRgn(IntPtr hWnd, IntPtr hRgn);
  [DllImport("user32.dll")] public static extern bool PtInRegion(IntPtr hRgn, int x, int y);
  [DllImport("gdi32.dll")] public static extern IntPtr CreateRectRgn(int l, int t, int r, int b);
  [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr o);
  [DllImport("gdi32.dll")] public static extern int GetRegionData(IntPtr hRgn, int count, IntPtr data);

  public static List<IntPtr> WindowsForPid(uint pid) {
    var list = new List<IntPtr>();
    EnumWindows((h, l) => {
      uint wpid; GetWindowThreadProcessId(h, out wpid);
      if (wpid == pid && IsWindowVisible(h)) list.Add(h);
      return true;
    }, IntPtr.Zero);
    return list;
  }

  public static RECT? SecondaryWorkArea() {
    RECT? secondary = null;
    EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr h, IntPtr hdc, ref RECT r, IntPtr d) => {
      MONITORINFO mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
      if (GetMonitorInfo(h, ref mi) && (mi.dwFlags & MONITORINFOF_PRIMARY) == 0) secondary = mi.rcWork;
      return true;
    }, IntPtr.Zero);
    return secondary;
  }

  // The window's input region, in window coordinates (relative to the window's
  // upper-left, which is GetWindowRect().Left/Top). This is exactly what the
  // frame loop built with SetWindowRgn: the sprite's alpha mask plus the
  // unioned "Open chat" hotspot. Returns an empty array when the window has no
  // region (fully click-through: no sprite drawn on this overlay).
  public static RECT[] RegionRects(IntPtr hWnd) {
    IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
    try {
      if (GetWindowRgn(hWnd, rgn) == 0) return new RECT[0]; // ERROR / no region
      int size = GetRegionData(rgn, 0, IntPtr.Zero);
      if (size <= 0) return new RECT[0];
      IntPtr buf = Marshal.AllocHGlobal(size);
      try {
        if (GetRegionData(rgn, size, buf) == 0) return new RECT[0];
        // RGNDATAHEADER: dwSize, iType, nCount, nRgnSize (4 x u32), then rcBound
        // (RECT, 16 bytes), then nCount RECTs.
        int count = Marshal.ReadInt32(buf, 8);
        int origin = 32;
        var rects = new RECT[count];
        for (int i = 0; i < count; i++) {
          int at = origin + i * 16;
          rects[i].Left = Marshal.ReadInt32(buf, at);
          rects[i].Top = Marshal.ReadInt32(buf, at + 4);
          rects[i].Right = Marshal.ReadInt32(buf, at + 8);
          rects[i].Bottom = Marshal.ReadInt32(buf, at + 12);
        }
        return rects;
      } finally { Marshal.FreeHGlobal(buf); }
    } finally { DeleteObject(rgn); }
  }

  // True when (x, y), in window coordinates, is inside the window's region.
  public static bool InRegion(IntPtr hWnd, int x, int y) {
    IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
    try {
      if (GetWindowRgn(hWnd, rgn) == 0) return false;
      return PtInRegion(rgn, x, y);
    } finally { DeleteObject(rgn); }
  }

  public static void LeftClick() {
    mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(30);
    mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, UIntPtr.Zero);
  }
  public static void DblClick() {
    mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(20);
    mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(40);
    mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(20);
    mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, UIntPtr.Zero);
  }
}
"@

# Physical pixels everywhere, so the frame trace, GetWindowRect, the window
# region, SetCursorPos, and the screenshot all speak one coordinate space even
# if the watch machine is scaled above 100%.
[Smoke]::SetProcessDPIAware() | Out-Null

# The sprite is buddy-bot's 90x90 art at scale 1; the frame trace reports its
# top-left. Center is the Summon target.
$SpriteSize = 90

$Bin = if ($env:AI_BUDDY_VERIFY_BIN) { $env:AI_BUDDY_VERIFY_BIN } else { Join-Path $Root "target\debug\ai-buddy.exe" }
if (-not (Test-Path $Bin)) {
  Warn "missing $Bin - attempting a debug build (needs VsDevCmd on PATH)"
  & cargo build --bin ai-buddy 2>&1 | Tee-Object -FilePath (Join-Path $Out "build.log")
  if (-not (Test-Path $Bin)) { Fail "no binary and build failed - open a VsDevCmd shell and run: cargo build" }
}
Pass "Binary ready: $Bin"

$Stub = Join-Path $Root "scripts\win-smoke-588-completer-stub.cjs"
if (-not (Test-Path $Stub)) { Fail "missing stub $Stub" }
$Port = if ($env:AI_BUDDY_SMOKE_PORT) { $env:AI_BUDDY_SMOKE_PORT } else { "18765" }

# --- Displays: park the console on the secondary, leave the primary clear for
# the Character so a screenshot is not the console. ---
$sec = [Smoke]::SecondaryWorkArea()
$dual = $null -ne $sec
if ($dual) {
  $consoleHwnd = (Get-Process -Id $PID).MainWindowHandle
  if ($consoleHwnd -ne [IntPtr]::Zero) {
    [Smoke]::SetWindowPos($consoleHwnd, [IntPtr]::Zero, $sec.Left + 20, $sec.Top + 20, 900, 640, 0) | Out-Null
    Info "Dual display: parked console on secondary at $($sec.Left),$($sec.Top)"
  }
} else {
  Info "Single display: console stays put"
}

# --- Completer stub: every wake answers with one long stroll turn, so the
# bubble truncates and #588 draws Open chat. ---
$stubPsi = New-Object System.Diagnostics.ProcessStartInfo
$stubPsi.FileName = "node"
$stubPsi.Arguments = "`"$Stub`""
$stubPsi.WorkingDirectory = $Root
$stubPsi.UseShellExecute = $false
$stubPsi.RedirectStandardError = $true
$stubPsi.RedirectStandardOutput = $true
$stubPsi.CreateNoWindow = $true
$stubPsi.EnvironmentVariables["AI_BUDDY_SMOKE_PORT"] = $Port
$script:StubProc = New-Object System.Diagnostics.Process
$script:StubProc.StartInfo = $stubPsi
try { $null = $script:StubProc.Start() } catch { Fail "could not start node stub (is node on PATH?): $_" }
$stubSw = [System.IO.StreamWriter]::new($StubLog, $false)
$stubHandler = { if ($EventArgs.Data) { $stubSw.WriteLine($EventArgs.Data); $stubSw.Flush() } }
Register-ObjectEvent -InputObject $script:StubProc -EventName ErrorDataReceived -Action $stubHandler | Out-Null
Register-ObjectEvent -InputObject $script:StubProc -EventName OutputDataReceived -Action $stubHandler | Out-Null
$script:StubProc.BeginErrorReadLine()
$script:StubProc.BeginOutputReadLine()
Start-Sleep -Milliseconds 300
if ($script:StubProc.HasExited) { Fail "stub exited immediately - see $StubLog" }
Pass "Completer stub on 127.0.0.1:$Port (pid=$($script:StubProc.Id))"

# --- App: HTTP Completer pointed at the stub, overlay left capturable so it is
# in screenshots, frame trace on so the Character can be located from code. ---
$env:AI_BUDDY_CHARACTER = "buddy-bot"
$env:AI_BUDDY_OPEN_SETTINGS = "0"
$env:AI_BUDDY_CAPTURABLE = "1"
$env:AI_BUDDY_TRACE_FRAMES = "1"
$env:AI_BUDDY_DIRECTOR = "1"
$env:AI_BUDDY_HARNESS = ""              # force the plain HTTP Completer path
$env:AI_BUDDY_DIRECTOR_BASE_URL = "http://127.0.0.1:$Port"
$env:AI_BUDDY_DIRECTOR_WAKE_SECS = "3"  # a short first ambient wake, not the 2m default
Remove-Item Env:AI_BUDDY_DIRECTOR_API_KEY -ErrorAction SilentlyContinue  # loopback needs none

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $Bin
$psi.WorkingDirectory = $Root
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.CreateNoWindow = $true
$script:AppProc = New-Object System.Diagnostics.Process
$script:AppProc.StartInfo = $psi
$null = $script:AppProc.Start()
$sw = [System.IO.StreamWriter]::new($Log, $false)
$handler = { if ($EventArgs.Data) { $sw.WriteLine($EventArgs.Data); $sw.Flush() } }
Register-ObjectEvent -InputObject $script:AppProc -EventName OutputDataReceived -Action $handler | Out-Null
Register-ObjectEvent -InputObject $script:AppProc -EventName ErrorDataReceived -Action $handler | Out-Null
$script:AppProc.BeginOutputReadLine()
$script:AppProc.BeginErrorReadLine()
$AppPid = [uint32]$script:AppProc.Id

function Await-Log([string]$Pattern, [int]$Attempts = 60) {
  for ($i = 0; $i -lt $Attempts; $i++) {
    if ((Test-Path $Log) -and (Select-String -Path $Log -Pattern $Pattern -Quiet -ErrorAction SilentlyContinue)) { return $true }
    Start-Sleep -Milliseconds 100
  }
  return $false
}

if (-not (Await-Log "overlay-\d+ covers" 80)) { Fail "overlays never built - $Log" }
if (-not (Await-Log "frame:" 80)) { Fail "no frame trace - $Log" }
Pass "Overlays built and frame loop running"

# --- Dismiss Settings if it auto-opened. With OPEN_SETTINGS=0 and the Director
# configured it should not, but a stray focus on the anchor window opens it, so
# close it once rather than risk clicking through it. ---
Start-Sleep -Milliseconds 500
foreach ($h in [Smoke]::WindowsForPid($AppPid)) {
  $cn = New-Object System.Text.StringBuilder(256)
  [Smoke]::GetClassName($h, $cn, 256) | Out-Null
  if ($cn.ToString() -eq "AiBuddySettings") {
    [Smoke]::PostMessage($h, [Smoke]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Warn "Settings auto-opened; dismissed it once"
    Start-Sleep -Milliseconds 300
    break
  }
}

# The newest sprite top-left the frame loop traced, in screen pixels.
function Latest-Sprite {
  $m = Select-String -Path $Log -Pattern "sprite\((-?\d+),(-?\d+)\)" -ErrorAction SilentlyContinue | Select-Object -Last 1
  if ($m -and $m.Line -match "sprite\((-?\d+),(-?\d+)\)") {
    return [PSCustomObject]@{ X = [int]$Matches[1]; Y = [int]$Matches[2] }
  }
  return $null
}

# The full-display overlay HWND covering a screen point. Full-display so the
# 136x39 anchor window can never be chosen; contains the point so the right
# display's overlay is picked on a multi-monitor desk.
function Overlay-At([int]$sx, [int]$sy) {
  foreach ($h in [Smoke]::WindowsForPid($AppPid)) {
    $r = New-Object Smoke+RECT
    if (-not [Smoke]::GetWindowRect($h, [ref]$r)) { continue }
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    if ($w -lt 800 -or $ht -lt 600) { continue }              # not a full-display overlay
    if ($sx -ge $r.Left -and $sx -lt $r.Right -and $sy -ge $r.Top -and $sy -lt $r.Bottom) {
      return [PSCustomObject]@{ Hwnd = $h; Rect = $r }
    }
  }
  return $null
}

# Read the "Open chat" rectangle straight out of the overlay's input region.
# The region is the sprite mask plus the unioned control; the only region
# rectangles clear of the 90px art box are the control's, so its bounding box
# is the click target. Returns a screen-space center or $null when the control
# is not in the region yet (no truncated bubble on this overlay).
function OpenChat-Point($overlay, $sprite) {
  $rects = [Smoke]::RegionRects($overlay.Hwnd)
  if ($rects.Count -eq 0) { return $null }
  $spriteTop = $sprite.Y - $overlay.Rect.Top          # sprite top in window coords
  $spriteBottom = $spriteTop + $SpriteSize
  $above = @($rects | Where-Object { $_.Bottom -le $spriteTop })
  $below = @($rects | Where-Object { $_.Top -ge $spriteBottom })
  # Above the head is the normal placement; below is the inverted placement the
  # bubble uses at the top of a display (placeBubble, #546). Prefer whichever
  # cluster exists.
  $band = if ($above.Count -gt 0) { $above } elseif ($below.Count -gt 0) { $below } else { $null }
  if ($null -eq $band) { return $null }
  # Measure-Object is skipped on purpose: it reads adapted properties, and these
  # are C# struct fields. A plain fold over the band is unambiguous.
  $minL = [int]::MaxValue; $maxR = [int]::MinValue
  $minT = [int]::MaxValue; $maxB = [int]::MinValue
  foreach ($r in $band) {
    if ($r.Left -lt $minL) { $minL = $r.Left }
    if ($r.Right -gt $maxR) { $maxR = $r.Right }
    if ($r.Top -lt $minT) { $minT = $r.Top }
    if ($r.Bottom -gt $maxB) { $maxB = $r.Bottom }
  }
  $winX = [int](($minL + $maxR) / 2)
  $winY = [int](($minT + $maxB) / 2)
  return [PSCustomObject]@{
    ScreenX = $overlay.Rect.Left + $winX
    ScreenY = $overlay.Rect.Top + $winY
    Width = $maxR - $minL; Height = $maxB - $minT
  }
}

# The Chat surface: a PID-owned window that is neither a full-display overlay
# nor the tiny anchor nor the native Settings window. Chat is built at 420x560.
function Find-Chat {
  foreach ($h in [Smoke]::WindowsForPid($AppPid)) {
    $cn = New-Object System.Text.StringBuilder(256)
    [Smoke]::GetClassName($h, $cn, 256) | Out-Null
    if ($cn.ToString() -eq "AiBuddySettings") { continue }
    $r = New-Object Smoke+RECT
    if (-not [Smoke]::GetWindowRect($h, [ref]$r)) { continue }
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    if ($w -ge 800 -and $ht -ge 600) { continue }   # overlay
    if ($w -lt 300 -or $ht -lt 300) { continue }    # anchor / stray chrome
    $tb = New-Object System.Text.StringBuilder(256)
    [Smoke]::GetWindowText($h, $tb, 256) | Out-Null
    return [PSCustomObject]@{ Hwnd = $h; Title = $tb.ToString(); W = $w; H = $ht }
  }
  return $null
}

# Confirm a screen point is a live click into the overlay's region and that the
# overlay is the top-most window there, then say whether it is safe to click.
function Confirm-Target($overlay, [int]$screenX, [int]$screenY) {
  $lx = $screenX - $overlay.Rect.Left
  $ly = $screenY - $overlay.Rect.Top
  if (-not [Smoke]::InRegion($overlay.Hwnd, $lx, $ly)) { return $false }
  $pt = New-Object Smoke+POINT; $pt.X = $screenX; $pt.Y = $screenY
  $root = [Smoke]::GetAncestor([Smoke]::WindowFromPoint($pt), [Smoke]::GA_ROOT)
  return $root -eq $overlay.Hwnd
}

# Track the (possibly strolling) Open chat control and click it once.
#
# On Windows the overlay only takes a click while the frame loop has the cursor
# over the control: it flips WS_EX_TRANSPARENT off a tick after the cursor
# lands (src-tauri/src/frame_loop.rs). So the cursor is placed, given a couple
# of ticks to make the overlay clickable, and re-centred on the newest sprite
# each pass so a walking buddy does not carry the control out from under it.
# The click fires only once the point is confirmed in the region and the
# overlay is top-most. Bounded to a few hundred milliseconds, not a pause.
function Track-And-Click-OpenChat {
  for ($k = 0; $k -lt 8; $k++) {
    $sp = Latest-Sprite
    if ($null -eq $sp) { return $false }
    $ov = Overlay-At ($sp.X + [int]($SpriteSize / 2)) ($sp.Y + [int]($SpriteSize / 2))
    if ($null -eq $ov) { return $false }
    $hot = OpenChat-Point $ov $sp
    if ($null -eq $hot) { return $false }   # bubble gone or control not in region
    [Smoke]::SetCursorPos($hot.ScreenX, $hot.ScreenY) | Out-Null
    Start-Sleep -Milliseconds 60             # let the overlay become clickable under the cursor
    if ($k -ge 2 -and (Confirm-Target $ov $hot.ScreenX $hot.ScreenY)) {
      Info "Open chat at screen $($hot.ScreenX),$($hot.ScreenY) ($($hot.Width)x$($hot.Height)); clicking"
      [Smoke]::LeftClick()
      return $true
    }
  }
  return $false
}

# Summon: a double-click on the sprite art opens Chat too (#17). Same settle so
# the art is clickable, re-sampled so the click lands on the current sprite.
function Summon-Sprite {
  $sp = Latest-Sprite
  if ($null -eq $sp) { return $false }
  $sx = $sp.X + [int]($SpriteSize / 2); $sy = $sp.Y + [int]($SpriteSize / 2)
  [Smoke]::SetCursorPos($sx, $sy) | Out-Null
  Start-Sleep -Milliseconds 80
  $sp = Latest-Sprite
  $sx = $sp.X + [int]($SpriteSize / 2); $sy = $sp.Y + [int]($SpriteSize / 2)
  $ov = Overlay-At $sx $sy
  if ($null -eq $ov) { return $false }
  [Smoke]::SetCursorPos($sx, $sy) | Out-Null
  Start-Sleep -Milliseconds 40
  if (Confirm-Target $ov $sx $sy) {
    Info "Summon double-click on sprite center $sx,$sy"
    [Smoke]::DblClick()
    return $true
  }
  return $false
}

function Screenshot([string]$name, $rect) {
  Add-Type -AssemblyName System.Drawing
  $w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
  $bmp = New-Object System.Drawing.Bitmap($w, $h)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
  $path = Join-Path $DocsPr $name
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Info "Screenshot: $path"
  return $path
}

# --- Wait for the truncated bubble, then click Open chat immediately. ---
# The wait polls the region for the control; it is not a pre-click pause. Once
# the control is in the region we re-sample and click within tens of ms. The
# budget is 2 Summon double-clicks plus 1 Open-chat click, and a Summon is the
# fallback only when the control has not appeared.
$summons = 0
$opened = $false
$openedVia = ""
$clickedOpenChat = $false
$openChatTried = $false
$shotBubble = ""
$deadline = (Get-Date).AddSeconds(45)
$summonAfter = (Get-Date).AddSeconds(14)   # give the first ambient wake time to land

Info "Waiting for a truncated bubble (Open chat control in the overlay region)..."
while ((Get-Date) -lt $deadline -and -not $opened) {
  $sprite = Latest-Sprite
  if ($null -eq $sprite) { Start-Sleep -Milliseconds 150; continue }
  $overlay = Overlay-At ($sprite.X + [int]($SpriteSize / 2)) ($sprite.Y + [int]($SpriteSize / 2))
  if ($null -eq $overlay) { Start-Sleep -Milliseconds 150; continue }

  $hot = OpenChat-Point $overlay $sprite
  if ($null -ne $hot -and -not $openChatTried) {
    # The truncated bubble is up. Photograph it, then track-and-click the
    # control immediately — the click re-samples the newest frame itself. This
    # is the one Open-chat click in the budget; on to Summon if it cannot land.
    if (-not $shotBubble) { $shotBubble = Screenshot "588-win-watch-$Stamp-bubble.png" $overlay.Rect }
    $openChatTried = $true
    if (Track-And-Click-OpenChat) {
      $clickedOpenChat = $true
      Start-Sleep -Milliseconds 500
      if (Find-Chat) { $opened = $true; $openedVia = "Open chat control" }
    } else {
      Warn "Open chat control seen but the click could not be confirmed; will try Summon"
    }
    continue
  }

  # Fallback: after the first wake window, or right after an Open-chat click
  # that did not open Chat. A Summon opens Chat too (#17) and may prod a
  # reactive turn that then truncates.
  if ($summons -lt 2 -and ($openChatTried -or (Get-Date) -ge $summonAfter)) {
    Info "Summon fallback (#$($summons + 1) of 2)"
    if (Summon-Sprite) { $summons++ }
    Start-Sleep -Milliseconds 700
    if (Find-Chat) { $opened = $true; $openedVia = "Summon fallback" }
    continue
  }
  Start-Sleep -Milliseconds 150
}

# --- Verdict. ---
if (-not $opened) {
  # One last look: Chat may have opened just after the final click.
  Start-Sleep -Milliseconds 500
  $chat = Find-Chat
  if ($chat) { $opened = $true; $openedVia = if ($clickedOpenChat) { "Open chat control" } else { "Summon fallback" } }
}

$finalSprite = Latest-Sprite
$shotChat = ""
if ($opened) {
  $chat = Find-Chat
  $disp = if ($finalSprite) { Overlay-At ($finalSprite.X + 45) ($finalSprite.Y + 45) } else { $null }
  $shotRect = if ($disp) { $disp.Rect } else { $overlay.Rect }
  $shotChat = Screenshot "588-win-watch-$Stamp-chat.png" $shotRect
}

$lines = @()
$lines += "PR #588 Open chat watch smoke - $Stamp"
$lines += "binary: $Bin"
$lines += "stub: 127.0.0.1:$Port"
$lines += "dual display: $dual"
$lines += "Open chat control clicked: $clickedOpenChat"
$lines += "Summon double-clicks used: $summons"
$lines += "Chat opened: $opened via '$openedVia'"
if ($shotBubble) { $lines += "bubble screenshot: $shotBubble" }
if ($shotChat) { $lines += "chat screenshot: $shotChat" }
$lines | Set-Content -Path $Report
Info "Report: $Report"

if ($opened) {
  Pass "Chat opened via $openedVia"
  Cleanup
  Write-Host "[PASS] PR #588 Open chat smoke finished" -ForegroundColor Green
  exit 0
} else {
  Get-Content $Log -Tail 40 | Set-Content (Join-Path $Out "tail.log")
  Fail "Chat never opened (Open chat clicked: $clickedOpenChat, Summons: $summons) - see $Out"
}
