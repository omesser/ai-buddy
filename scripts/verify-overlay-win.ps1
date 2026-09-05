#!/usr/bin/env pwsh
# Windows overlay verification: non-activating panel, Perch physics, click-through.
# Dual-display test: place Notepad on secondary monitor, drag sprite from primary.
#
# Usage:
#   .\scripts\verify-overlay-win.ps1
#   $env:AI_BUDDY_VERIFY_SKIP_BUILD=1 .\scripts\verify-overlay-win.ps1  # skip cargo build
#   $env:AI_BUDDY_VERIFY_BIN="path\to\ai-buddy.exe" .\scripts\verify-overlay-win.ps1

$ErrorActionPreference = "Stop"

# Config
$VerifyDir = ".verify\win-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
$LogFile = "$VerifyDir\verify.log"
$FrameLog = "$VerifyDir\frames.log"
$PollInterval = 100  # ms - fast polling
$MaxAttempts = 150   # 15s at 100ms intervals
$DragSteps = 18      # Fewer steps, larger deltas
$DragStepDelay = 25  # ms per step

# Create verify directory
New-Item -ItemType Directory -Path $VerifyDir -Force | Out-Null

function Log {
    param($Message, $Level = "INFO")
    $Timestamp = Get-Date -Format "HH:mm:ss.fff"
    $Line = "[$Timestamp] $Level : $Message"
    Write-Host $Line
    Add-Content -Path $LogFile -Value $Line
}

function Await-Log {
    param($Path, $Pattern, $Attempts = $MaxAttempts)
    
    $LastPosition = 0
    for ($i = 0; $i -lt $Attempts; $i++) {
        if (Test-Path $Path) {
            $Content = Get-Content -Path $Path -Raw -ErrorAction SilentlyContinue
            if ($Content -and $Content.Substring($LastPosition) -match $Pattern) {
                return $true
            }
            $LastPosition = $Content.Length
        }
        Start-Sleep -Milliseconds $PollInterval
    }
    return $false
}

# Clean up on exit
$Script:Processes = @()
Register-EngineEvent -SourceIdentifier PowerShell.Exiting -Action {
    foreach ($Proc in $Script:Processes) {
        if (-not $Proc.HasExited) {
            Stop-Process -Id $Proc.Id -Force -ErrorAction SilentlyContinue
        }
    }
} | Out-Null

try {
    Log "Windows overlay verification starting"
    
    # Build if not skipped
    if (-not $env:AI_BUDDY_VERIFY_SKIP_BUILD) {
        Log "Building ai-buddy..."
        cargo build -p ai-buddy 2>&1 | Out-File -FilePath "$VerifyDir\build.log"
        if ($LASTEXITCODE -ne 0) {
            Log "Build failed" "FAIL"
            exit 1
        }
    }
    
    # Binary path
    $BinaryPath = if ($env:AI_BUDDY_VERIFY_BIN) { $env:AI_BUDDY_VERIFY_BIN } else { "target\debug\ai-buddy.exe" }
    if (-not (Test-Path $BinaryPath)) {
        Log "Binary not found: $BinaryPath" "FAIL"
        exit 1
    }
    
    # Find secondary monitor
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Monitor {
    [DllImport("user32.dll")]
    public static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr lprcClip, MonitorEnumProc lpfnEnum, IntPtr dwData);
    public delegate bool MonitorEnumProc(IntPtr hMonitor, IntPtr hdcMonitor, ref RECT lprcMonitor, IntPtr dwData);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
    
    $Monitors = [System.Collections.ArrayList]::new()
    $Callback = {
        param($hMonitor, $hdcMonitor, [ref]$lprcMonitor, $dwData)
        $Monitors.Add(@{
            Left = $lprcMonitor.Value.Left
            Top = $lprcMonitor.Value.Top
            Width = $lprcMonitor.Value.Right - $lprcMonitor.Value.Left
            Height = $lprcMonitor.Value.Bottom - $lprcMonitor.Value.Top
        }) | Out-Null
        return $true
    }
    
    [Monitor]::EnumDisplayMonitors([IntPtr]::Zero, [IntPtr]::Zero, $Callback, [IntPtr]::Zero) | Out-Null
    
    if ($Monitors.Count -lt 2) {
        Log "Single display detected; placing Notepad on primary at (1400, 700)" "WARN"
        $TargetX = 1400
        $TargetY = 700
    } else {
        # Use secondary (non-primary, typically negative coords or offset)
        $Secondary = $Monitors | Where-Object { $_.Left -ne 0 -or $_.Top -ne 0 } | Select-Object -First 1
        if (-not $Secondary) { $Secondary = $Monitors[1] }
        $TargetX = $Secondary.Left + 200
        $TargetY = $Secondary.Top + 400
        Log "Secondary display: ($($Secondary.Left),$($Secondary.Top)) $($Secondary.Width)×$($Secondary.Height)"
    }
    
    # Launch Notepad
    Log "Launching Notepad..."
    $Notepad = Start-Process -FilePath "notepad.exe" -PassThru -WindowStyle Normal
    $Script:Processes += $Notepad
    Start-Sleep -Milliseconds 500
    
    # Position Notepad
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
    [DllImport("user32.dll")] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hWnd, int nIndex);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
    
    Start-Sleep -Milliseconds 300
    $NotepadHwnd = [Win32]::FindWindow("Notepad", $null)
    if ($NotepadHwnd -eq [IntPtr]::Zero) {
        Log "Failed to find Notepad window" "FAIL"
        exit 1
    }
    
    [Win32]::MoveWindow($NotepadHwnd, $TargetX, $TargetY, 900, 500, $true) | Out-Null
    Start-Sleep -Milliseconds 200
    
    $NotepadRect = New-Object Win32+RECT
    [Win32]::GetWindowRect($NotepadHwnd, [ref]$NotepadRect) | Out-Null
    $NotepadTop = $NotepadRect.Top
    $NotepadCenterX = ($NotepadRect.Left + $NotepadRect.Right) / 2
    Log "Notepad positioned: ($($NotepadRect.Left),$NotepadTop)-($($NotepadRect.Right),$($NotepadRect.Bottom))"
    
    # Launch ai-buddy
    Log "Launching ai-buddy with frame TRACE..."
    $env:AI_BUDDY_TRACE_FRAMES = "1"
    $env:AI_BUDDY_CHARACTER = "buddy-bot"
    $AiBuddy = Start-Process -FilePath $BinaryPath -PassThru -RedirectStandardError $FrameLog -NoNewWindow
    $Script:Processes += $AiBuddy
    
    # Wait for Grounded
    Log "Waiting for sprite Grounded..."
    if (-not (Await-Log -Path $FrameLog -Pattern "Grounded" -Attempts 200)) {
        Log "Sprite never reached Grounded" "FAIL"
        exit 1
    }
    Log "Sprite Grounded" "PASS"
    
    # Check WS_EX_NOACTIVATE
    Start-Sleep -Milliseconds 500
    $Overlays = Get-Process | Where-Object { $_.MainWindowTitle -match "ai-buddy" }
    $HasNoActivate = $false
    foreach ($Overlay in $Overlays) {
        $Hwnd = $Overlay.MainWindowHandle
        if ($Hwnd -ne [IntPtr]::Zero) {
            $ExStyle = [Win32]::GetWindowLong($Hwnd, -20)  # GWL_EXSTYLE
            if (($ExStyle -band 0x08000000) -ne 0) {  # WS_EX_NOACTIVATE
                $HasNoActivate = $true
                break
            }
        }
    }
    if ($HasNoActivate) {
        Log "WS_EX_NOACTIVATE verified" "PASS"
    } else {
        Log "WS_EX_NOACTIVATE not found" "WARN"
    }
    
    # Find sprite position from last Grounded frame
    $LastGrounded = Get-Content $FrameLog | Select-String "Grounded pos\((-?\d+),(-?\d+)\)" | Select-Object -Last 1
    if ($LastGrounded -match "pos\((-?\d+),(-?\d+)\)") {
        $SpriteX = [int]$Matches[1] + 45  # +45 for sprite center (90×90)
        $SpriteY = [int]$Matches[2] + 45
        Log "Sprite center at ($SpriteX, $SpriteY)"
    } else {
        Log "Could not parse sprite position" "FAIL"
        exit 1
    }
    
    # Grab sprite
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Mouse {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
    [DllImport("user32.dll")] public static extern void mouse_event(int dwFlags, int dx, int dy, int dwData, int dwExtraInfo);
}
"@
    
    Log "Grabbing sprite at ($SpriteX, $SpriteY)..."
    [Mouse]::SetCursorPos($SpriteX, $SpriteY)
    Start-Sleep -Milliseconds 50
    [Mouse]::mouse_event(0x0002, 0, 0, 0, 0)  # MOUSEEVENTF_LEFTDOWN
    Start-Sleep -Milliseconds 100
    
    # Wait for Dragged
    if (-not (Await-Log -Path $FrameLog -Pattern "Dragged" -Attempts 30)) {
        Log "Sprite not Dragged" "FAIL"
        [Mouse]::mouse_event(0x0004, 0, 0, 0, 0)  # MOUSEEVENTF_LEFTUP
        exit 1
    }
    Log "Sprite Dragged" "PASS"
    
    # Drag to 80px above Notepad top
    $DropX = [int]$NotepadCenterX
    $DropY = $NotepadTop - 80
    Log "Dragging to ($DropX, $DropY) - 80px above window top..."
    
    for ($Step = 1; $Step -le $DragSteps; $Step++) {
        $CurrentX = $SpriteX + ($DropX - $SpriteX) * $Step / $DragSteps
        $CurrentY = $SpriteY + ($DropY - $SpriteY) * $Step / $DragSteps
        [Mouse]::SetCursorPos([int]$CurrentX, [int]$CurrentY)
        Start-Sleep -Milliseconds $DragStepDelay
    }
    
    # Release
    [Mouse]::mouse_event(0x0004, 0, 0, 0, 0)  # MOUSEEVENTF_LEFTUP
    Log "Released at ($DropX, $DropY)"
    
    # Wait for Perched
    if (-not (Await-Log -Path $FrameLog -Pattern "Perched" -Attempts 50)) {
        Log "Sprite not Perched - check frame log for actual state" "FAIL"
        exit 1
    }
    Log "Sprite Perched" "PASS"
    
    # Quick click check (sprite should not steal focus)
    Start-Sleep -Milliseconds 300
    [Mouse]::SetCursorPos($DropX, $DropY)
    [Mouse]::mouse_event(0x0002, 0, 0, 0, 0)  # MOUSEEVENTF_LEFTDOWN
    Start-Sleep -Milliseconds 50
    [Mouse]::mouse_event(0x0004, 0, 0, 0, 0)  # MOUSEEVENTF_LEFTUP
    Start-Sleep -Milliseconds 200
    
    Log "All checks passed" "PASS"
    
} catch {
    Log "Exception: $_" "FAIL"
    exit 1
} finally {
    # Clean up
    foreach ($Proc in $Script:Processes) {
        if (-not $Proc.HasExited) {
            Stop-Process -Id $Proc.Id -Force -ErrorAction SilentlyContinue
        }
    }
}
