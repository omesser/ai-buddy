#!/usr/bin/env pwsh
<#
.SYNOPSIS
Drives and reads the Windows Settings window through UI Automation.

.DESCRIPTION
Why this exists: the Settings window is native Win32, so the only way to check it
from outside was direct Win32 API calls. UI Automation (UIA) provides a higher-level
accessibility API that can inspect window structure, read control states (enabled/
disabled), and interact with controls. Every question the verification asks - is this
row frozen, do the sections come in this order, is this disclosure expanded - is a
string or boolean UIA already exposes.

Why not other tools: UI Automation is the native Windows accessibility protocol for
Win32 applications. Other tools either don't work with raw Win32 or require
extensive setup.

.PARAMETER Command
The command to execute: wait, dump, pick-source, expand-disclosure

.PARAMETER Timeout
For wait command: timeout in seconds (default: 30)

.PARAMETER Title
For pick-source command: the AI source title to pick from the combo box

.PARAMETER DisclosureLabel
For expand-disclosure command: the label of the disclosure to expand

.EXAMPLE
.\ax-settings-win.ps1 -Command wait -Timeout 30

.EXAMPLE
.\ax-settings-win.ps1 -Command dump

.EXAMPLE
.\ax-settings-win.ps1 -Command pick-source -Title "Harness $([char]0x00B7) claude"

.EXAMPLE
.\ax-settings-win.ps1 -Command expand-disclosure -DisclosureLabel "What is this?"

.NOTES
Needs Windows PowerShell 5.1 or PowerShell 7+ with access to UI Automation APIs.
#>

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('wait', 'dump', 'pick-source', 'expand-disclosure')]
    [string]$Command,

    [int]$Timeout = 30,
    [string]$Title,
    [string]$DisclosureLabel
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Find-SettingsWindow {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ClassNameProperty,
        "AiBuddySettings"
    )

    $window = $root.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        $condition
    )

    return $window
}

function Wait-ForSettings {
    param([int]$TimeoutSec)

    $waited = 0
    while ($waited -lt $TimeoutSec) {
        $window = Find-SettingsWindow
        if ($null -ne $window) {
            return 0
        }
        Start-Sleep -Seconds 1
        $waited++
    }
    return 1
}

function Get-ElementState {
    param($element)

    $states = @()
    try {
        if ($element.Current.IsEnabled) {
            $states += "enabled"
        } else {
            $states += "disabled"
        }

        if ($element.Current.IsOffscreen) {
            $states += "offscreen"
        }
    } catch {
    }

    return $states
}

function Dump-Element {
    param(
        $element,
        [int]$depth = 0
    )

    $indent = "  " * $depth
    $type = $element.Current.ControlType.ProgrammaticName -replace 'ControlType.', ''
    $name = $element.Current.Name
    $states = Get-ElementState $element
    $stateStr = if ($states.Count -gt 0) { "[$($states -join ',')]" } else { "" }

    Write-Output "${indent}${type}|${name}|${stateStr}"

    try {
        $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
        $child = $walker.GetFirstChild($element)

        while ($null -ne $child) {
            Dump-Element $child ($depth + 1)
            $child = $walker.GetNextSibling($child)
        }
    } catch {
    }
}

function Dump-Settings {
    $window = Find-SettingsWindow
    if ($null -eq $window) {
        Write-Error "Settings window not found"
        return 1
    }

    try {
        Dump-Element $window
        return 0
    } catch {
        Write-Error "UI Automation error: $_"
        return 1
    }
}

function Find-ComboBoxByOptions {
    param($window, [string[]]$requiredOptions)

    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::ComboBox
    )

    $combos = $window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        $condition
    )

    foreach ($combo in $combos) {
        try {
            $expandPattern = $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
            if ($null -ne $expandPattern) {
                $expandPattern.Expand()
                Start-Sleep -Milliseconds 200

                $itemCondition = New-Object System.Windows.Automation.PropertyCondition(
                    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                    [System.Windows.Automation.ControlType]::ListItem
                )

                $items = $combo.FindAll(
                    [System.Windows.Automation.TreeScope]::Descendants,
                    $itemCondition
                )

                $foundOptions = @()
                foreach ($item in $items) {
                    $foundOptions += $item.Current.Name
                }

                $hasAll = $true
                foreach ($req in $requiredOptions) {
                    if ($req -notin $foundOptions) {
                        $hasAll = $false
                        break
                    }
                }

                if ($hasAll) {
                    return $combo
                }

                $expandPattern.Collapse()
            }
        } catch {
        }
    }

    return $null
}

function Pick-Source {
    param([string]$SourceTitle)

    $window = Find-SettingsWindow
    if ($null -eq $window) {
        Write-Error "Settings window not found"
        return 1
    }

    $combo = Find-ComboBoxByOptions $window @("Model API")

    if ($null -eq $combo) {
        Write-Error "AI source combo box not found"
        return 1
    }

    try {
        $expandPattern = $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
        $expandPattern.Expand()
        Start-Sleep -Milliseconds 200

        $nameCondition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty,
            $SourceTitle
        )
        $typeCondition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::ListItem
        )
        $andCondition = New-Object System.Windows.Automation.AndCondition($nameCondition, $typeCondition)

        $item = $combo.FindFirst(
            [System.Windows.Automation.TreeScope]::Descendants,
            $andCondition
        )

        if ($null -eq $item) {
            Write-Error "Menu item not found: $SourceTitle"
            return 1
        }

        $selectionPattern = $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
        $selectionPattern.Select()

        Start-Sleep -Milliseconds 500
        return 0
    } catch {
        Write-Error "UI Automation error: $_"
        return 1
    }
}

function Expand-Disclosure {
    param([string]$Label)

    $window = Find-SettingsWindow
    if ($null -eq $window) {
        Write-Error "Settings window not found"
        return 1
    }

    $nameCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Label
    )
    $typeCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Button
    )
    $andCondition = New-Object System.Windows.Automation.AndCondition($nameCondition, $typeCondition)

    $button = $window.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        $andCondition
    )

    if ($null -eq $button) {
        Write-Error "Disclosure button not found: $Label"
        return 1
    }

    try {
        $invokePattern = $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $invokePattern.Invoke()

        Start-Sleep -Milliseconds 300
        return 0
    } catch {
        Write-Error "UI Automation error: $_"
        return 1
    }
}

switch ($Command) {
    'wait' {
        exit (Wait-ForSettings $Timeout)
    }
    'dump' {
        exit (Dump-Settings)
    }
    'pick-source' {
        if ([string]::IsNullOrEmpty($Title)) {
            Write-Error "-Title is required for pick-source command"
            exit 1
        }
        exit (Pick-Source $Title)
    }
    'expand-disclosure' {
        if ([string]::IsNullOrEmpty($DisclosureLabel)) {
            Write-Error "-DisclosureLabel is required for expand-disclosure command"
            exit 1
        }
        exit (Expand-Disclosure $DisclosureLabel)
    }
}
