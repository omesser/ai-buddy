# Dump one top-level window's UI Automation tree as `role|name` lines.
# EnumWindows plus FromHandle, not RootElement.FindFirst: a desktop-wide
# descendant search can hang on a multi-monitor session.
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('dump')]
    [string]$Command,
    [Parameter(Mandatory = $true)]
    [int]$ProcessId,
    [Parameter(Mandatory = $true)]
    [string]$Title
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class FidgetWinEnum {
    public delegate bool Callback(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lp, int n);
    public static IntPtr Find(uint pid, string title) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint owner;
            GetWindowThreadProcessId(h, out owner);
            if (owner != pid) return true;
            var sb = new StringBuilder(512);
            GetWindowText(h, sb, sb.Capacity);
            if (sb.ToString().IndexOf(title, StringComparison.OrdinalIgnoreCase) >= 0) {
                found = h;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
"@

function Write-Node {
    param($Element, [int]$Depth)
    if ($null -eq $Element -or $Depth -gt 30) { return }
    $role = "other"
    $name = ""
    try {
        $name = ($Element.Current.Name -replace "`n", "\n")
        if ($Element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button) {
            $role = "button"
        } elseif ($Element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Window) {
            $role = "frame"
        } elseif ($Element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Text) {
            $role = "label"
        } elseif ($Element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Hyperlink) {
            $role = "link"
        }
    } catch { return }
    Write-Output ("{0}|{1}" -f $role, $name)
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $child = $walker.GetFirstChild($Element)
    while ($null -ne $child) {
        Write-Node -Element $child -Depth ($Depth + 1)
        $child = $walker.GetNextSibling($child)
    }
}

$hwnd = [FidgetWinEnum]::Find([uint32]$ProcessId, $Title)
if ($hwnd -eq [IntPtr]::Zero) {
    Write-Error "no window titled $Title for pid $ProcessId"
    exit 1
}
$window = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
if ($null -eq $window) {
    Write-Error "FromHandle returned nothing for $hwnd"
    exit 1
}
Write-Node -Element $window -Depth 0
exit 0
