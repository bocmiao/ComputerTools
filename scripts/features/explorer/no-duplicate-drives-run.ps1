# Feature: explorer.no-duplicate-drives -- run (and prepare)
# Deletes the delegate folder key (see the shared block) in both views, so
# removable drives only show under This PC. Explorer windows opened afterwards
# show it; the engine offers to restart Explorer.
# -Prepare: returns before = { x64 = { exists, value }, x86 = { exists, value } }.
# Run: -Before is that JSON. Returns skipped (nothing changed) when things
#   changed since or when neither view has the key. Throws, changing nothing,
#   when a key holds more than its default value. Otherwise deletes the keys
#   and checks they are gone; when that fails, the deleted keys are put back
#   and the script throws.

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block no-duplicate-drives: identical in features/explorer/no-duplicate-drives-detect.ps1, no-duplicate-drives-run.ps1, no-duplicate-drives-undo.ps1 and no-duplicate-drives-break.ps1 (medkit-data check compares them) ----
# File Explorer's navigation pane lists removable drives (USB sticks, SD
# cards) twice: under This PC, and once more on their own. The second entry is
# a delegate folder registered under Desktop\NameSpace\DelegateFolders; without
# that key removable drives only show under This PC. Win11Debloat (Raphire,
# MIT) deletes the 64-bit key and brings it back with the default value
# 'Removable Drives'. The 32-bit view (WOW6432Node) has the same key for the
# open and save dialogs of 32-bit programs, so both views are handled. A key
# that holds anything besides its default value is left alone.
$drivesParentPath = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\DelegateFolders'
$drivesKeyName = '{F5FB2C77-0E2F-4A16-A381-3E560C68BC83}'
$drivesViews = @('x64', 'x86')

# DelegateFolders in the 64-bit ('x64') or 32-bit ('x86') view of HKLM, or
# $null when it does not exist.
function Open-DrivesParent {
    param([string]$View, [bool]$Writable)
    $registryView = [Microsoft.Win32.RegistryView]::Registry64
    if ($View -eq 'x86') {
        $registryView = [Microsoft.Win32.RegistryView]::Registry32
    }
    $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $registryView)
    try {
        return $base.OpenSubKey($drivesParentPath, $Writable)
    }
    finally {
        $base.Close()
    }
}

# The key in one view: Exists, Value (its default value; $null when missing or
# not text) and Plain (no subkeys and no values besides the default one).
function Get-DrivesKey {
    param([string]$View)
    $result = [pscustomobject]@{ Exists = $false; Value = $null; Plain = $true }
    $parent = Open-DrivesParent -View $View -Writable $false
    if ($null -eq $parent) {
        return $result
    }
    try {
        $key = $parent.OpenSubKey($drivesKeyName, $false)
        if ($null -eq $key) {
            return $result
        }
        try {
            $value = $key.GetValue('', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            if ($value -isnot [string]) {
                $value = $null
            }
            $others = @($key.GetValueNames() | Where-Object { $_.Length -gt 0 })
            $result.Exists = $true
            $result.Value = $value
            $result.Plain = (($key.SubKeyCount -eq 0) -and ($others.Count -eq 0))
        }
        finally {
            $key.Close()
        }
    }
    finally {
        $parent.Close()
    }
    return $result
}

# Deletes the key in one view (DeleteSubKey refuses a key with subkeys).
function Remove-DrivesKey {
    param([string]$View)
    $parent = Open-DrivesParent -View $View -Writable $true
    if ($null -eq $parent) {
        return
    }
    try {
        $parent.DeleteSubKey($drivesKeyName, $false)
    }
    finally {
        $parent.Close()
    }
}

# Creates the key in one view, with $Value as its default value ($null: none).
# Throws when that view has no DelegateFolders key.
function New-DrivesKey {
    param([string]$View, $Value)
    $parent = Open-DrivesParent -View $View -Writable $true
    if ($null -eq $parent) {
        throw ('there is no DelegateFolders key in the ' + $View + ' view')
    }
    try {
        $key = $parent.CreateSubKey($drivesKeyName)
        try {
            if ($null -ne $Value) {
                $key.SetValue('', [string]$Value, [Microsoft.Win32.RegistryValueKind]::String)
            }
        }
        finally {
            $key.Close()
        }
    }
    finally {
        $parent.Close()
    }
}
# ---- end of shared block no-duplicate-drives ----

$now = [ordered]@{}
$snapshot = [ordered]@{}
foreach ($view in $drivesViews) {
    $now[$view] = Get-DrivesKey -View $view
    $snapshot[$view] = [ordered]@{ exists = $now[$view].Exists; value = $now[$view].Value }
}
if ($Prepare) {
    return [pscustomobject]@{ before = $snapshot }
}

$recorded = ConvertFrom-Json -InputObject $Before
foreach ($view in $drivesViews) {
    $was = $recorded.$view
    if (([bool]$was.exists -ne $now[$view].Exists) -or ([string]$was.value -ne [string]$now[$view].Value)) {
        return [pscustomobject]@{ skipped = $true }
    }
}
$present = @($drivesViews | Where-Object { $now[$_].Exists })
if ($present.Count -eq 0) {
    return [pscustomobject]@{ skipped = $true }
}
foreach ($view in $present) {
    if (-not $now[$view].Plain) {
        throw ('the ' + $view + ' key holds more than its default value; left alone')
    }
}

$removed = @()
try {
    foreach ($view in $present) {
        Remove-DrivesKey -View $view
        $removed += $view
    }
    foreach ($view in $present) {
        if ((Get-DrivesKey -View $view).Exists) {
            throw ('the ' + $view + ' key is still there')
        }
    }
}
catch {
    $failure = $_
    # Put back what was deleted (the engine runs the undo script as well).
    foreach ($view in $removed) {
        try {
            if (-not (Get-DrivesKey -View $view).Exists) {
                New-DrivesKey -View $view -Value $now[$view].Value
            }
        }
        catch {
            Write-Verbose ('could not put the ' + $view + ' key back: ' + $_.Exception.Message)
        }
    }
    throw $failure
}

[pscustomobject]@{ after = [ordered]@{ x64 = [ordered]@{ exists = $false; value = $null }; x86 = [ordered]@{ exists = $false; value = $null } } }
