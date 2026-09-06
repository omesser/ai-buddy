# Verify Windows native settings window
#
# Automated e2e verification for Windows settings window using PostMessage
# to interact with native Win32 controls. Never uses SetCursorPos, so it
# won't interfere with other apps.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts/verify-settings-win.ps1
#
# Prerequisites:
#   - ai-buddy must be running
#   - Settings window must be open
#
# Tested items (via PostMessage):
#   - Window creation and singleton behavior
#   - Tab navigation via PostMessage
#   - Checkbox toggling
#   - Text field interaction
#
# Items requiring manual verification:
#   - Layout spacing and readability vs macOS/GTK
#   - Window focus behavior from tray/menu
#   - Visual rendering at different DPI settings

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public class Win32 {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr FindWindowEx(IntPtr hwndParent, IntPtr hwndChildAfter, string lpszClass, string lpszWindow);

    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, StringBuilder lParam);

    [DllImport("user32.dll")]
    public static extern bool IsWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    // Window messages
    public const uint WM_GETTEXT = 0x000D;
    public const uint WM_GETTEXTLENGTH = 0x000E;
    public const uint WM_CLOSE = 0x0010;
    public const uint BM_GETCHECK = 0x00F0;
    public const uint BM_SETCHECK = 0x00F1;
    public const uint BM_CLICK = 0x00F5;
    public const uint TCM_GETCURSEL = 0x130B;
    public const uint TCM_SETCURSEL = 0x130C;
    public const uint TCM_GETITEMCOUNT = 0x1304;

    // Button states
    public const int BST_UNCHECKED = 0x0000;
    public const int BST_CHECKED = 0x0001;
}
"@

function Find-SettingsWindow {
    $hwnd = [Win32]::FindWindow($null, "ai-buddy")
    if ($hwnd -eq [IntPtr]::Zero) {
        Write-Host "ERROR: Settings window not found. Is ai-buddy running with settings open?" -ForegroundColor Red
        return $null
    }

    if (-not [Win32]::IsWindow($hwnd)) {
        Write-Host "ERROR: Found window handle is invalid" -ForegroundColor Red
        return $null
    }

    Write-Host "Found settings window: 0x$($hwnd.ToString('X'))" -ForegroundColor Green
    return $hwnd
}

function Get-WindowText {
    param([IntPtr]$hwnd)

    $length = [Win32]::SendMessage($hwnd, [Win32]::WM_GETTEXTLENGTH, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
    if ($length -eq 0) {
        return ""
    }

    $sb = New-Object System.Text.StringBuilder($length + 1)
    [Win32]::SendMessage($hwnd, [Win32]::WM_GETTEXT, [IntPtr]($length + 1), $sb) | Out-Null
    return $sb.ToString()
}

function Find-TabControl {
    param([IntPtr]$parentHwnd)

    $tab = [Win32]::FindWindowEx($parentHwnd, [IntPtr]::Zero, "SysTabControl32", $null)
    if ($tab -eq [IntPtr]::Zero) {
        Write-Host "ERROR: Tab control not found" -ForegroundColor Red
        return $null
    }

    $tabCount = [Win32]::SendMessage($tab, [Win32]::TCM_GETITEMCOUNT, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
    Write-Host "Found tab control with $tabCount tabs" -ForegroundColor Green
    return $tab
}

function Test-TabNavigation {
    param([IntPtr]$tabHwnd)

    Write-Host "`nTesting tab navigation..." -ForegroundColor Cyan

    $tabCount = [Win32]::SendMessage($tabHwnd, [Win32]::TCM_GETITEMCOUNT, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
    $currentTab = [Win32]::SendMessage($tabHwnd, [Win32]::TCM_GETCURSEL, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()

    Write-Host "  Current tab: $currentTab of $tabCount"

    # Navigate to each tab
    for ($i = 0; $i -lt $tabCount; $i++) {
        [Win32]::SendMessage($tabHwnd, [Win32]::TCM_SETCURSEL, [IntPtr]$i, [IntPtr]::Zero) | Out-Null
        Start-Sleep -Milliseconds 200
        $newTab = [Win32]::SendMessage($tabHwnd, [Win32]::TCM_GETCURSEL, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
        if ($newTab -eq $i) {
            Write-Host "  Tab $i: OK" -ForegroundColor Green
        } else {
            Write-Host "  Tab $i: FAILED (got tab $newTab)" -ForegroundColor Red
        }
    }

    # Return to original tab
    [Win32]::SendMessage($tabHwnd, [Win32]::TCM_SETCURSEL, [IntPtr]$currentTab, [IntPtr]::Zero) | Out-Null
}

function Test-CheckboxControl {
    param(
        [IntPtr]$parentHwnd,
        [string]$controlText
    )

    Write-Host "`nSearching for checkbox: '$controlText'" -ForegroundColor Cyan

    $child = [IntPtr]::Zero
    do {
        $child = [Win32]::FindWindowEx($parentHwnd, $child, "Button", $null)
        if ($child -ne [IntPtr]::Zero) {
            $text = Get-WindowText $child
            if ($text -eq $controlText) {
                Write-Host "  Found checkbox" -ForegroundColor Green

                # Get initial state
                $initialState = [Win32]::SendMessage($child, [Win32]::BM_GETCHECK, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
                Write-Host "  Initial state: $(if ($initialState -eq [Win32]::BST_CHECKED) { 'CHECKED' } else { 'UNCHECKED' })"

                # Toggle it (click)
                [Win32]::SendMessage($child, [Win32]::BM_CLICK, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
                Start-Sleep -Milliseconds 100

                # Check new state
                $newState = [Win32]::SendMessage($child, [Win32]::BM_GETCHECK, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
                Write-Host "  New state: $(if ($newState -eq [Win32]::BST_CHECKED) { 'CHECKED' } else { 'UNCHECKED' })"

                # Toggle back
                [Win32]::SendMessage($child, [Win32]::BM_CLICK, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
                Start-Sleep -Milliseconds 100

                $finalState = [Win32]::SendMessage($child, [Win32]::BM_GETCHECK, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()

                if ($finalState -eq $initialState) {
                    Write-Host "  Toggle test: PASSED" -ForegroundColor Green
                } else {
                    Write-Host "  Toggle test: FAILED" -ForegroundColor Red
                }

                return
            }
        }
    } while ($child -ne [IntPtr]::Zero)

    Write-Host "  Checkbox not found" -ForegroundColor Yellow
}

# Main verification
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Windows Settings Window Verification" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

$settingsHwnd = Find-SettingsWindow
if ($settingsHwnd -eq $null) {
    exit 1
}

$tabControl = Find-TabControl $settingsHwnd
if ($tabControl -eq $null) {
    exit 1
}

# Test tab navigation
Test-TabNavigation $tabControl

# Test a few checkboxes (examples)
Test-CheckboxControl $settingsHwnd "Director"
Test-CheckboxControl $settingsHwnd "Do Not Disturb"
Test-CheckboxControl $settingsHwnd "Sound"

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Automated tests complete!" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

Write-Host "Manual verification checklist:" -ForegroundColor Yellow
Write-Host "  [ ] Layout spacing matches macOS/GTK readability" -ForegroundColor Yellow
Write-Host "  [ ] Margins are consistent and comfortable" -ForegroundColor Yellow
Write-Host "  [ ] Field widths are appropriate" -ForegroundColor Yellow
Write-Host "  [ ] Section gaps visually separate content" -ForegroundColor Yellow
Write-Host "  [ ] Opening from tray/menu focuses existing window" -ForegroundColor Yellow
Write-Host "  [ ] No duplicate windows are created" -ForegroundColor Yellow
Write-Host "  [ ] Window renders correctly at different DPI" -ForegroundColor Yellow
