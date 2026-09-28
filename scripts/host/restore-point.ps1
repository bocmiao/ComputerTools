#Requires -Version 5.1
<#
.SYNOPSIS
    Create a system restore point before a caution/danger level change.

.DESCRIPTION
    Windows creates at most one restore point per 24 hours through this API by
    default; a second request is skipped (Checkpoint-Computer only writes a
    warning). Microsoft's documented switch for that limit is the DWORD
    SystemRestorePointCreationFrequency under
    HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore: with 0,
    "System Restore does not skip creating the new restore point"
    (SRSetRestorePoint). It is set to 0 for this one call only and put back
    right after (removed when it was not there), so that every caution/danger
    change gets a fresh restore point.

    The number of restore points before and after still tells "created" from
    "skipped" (for example when the value cannot be written). Throws when
    System Restore is turned off.

    Result codes: created, skipped.
#>
[CmdletBinding()]
param(
    [string]$Description = 'medkit'
)

$ErrorActionPreference = 'Stop'

$restoreKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'
$frequencyName = 'SystemRestorePointCreationFrequency'

# The value as it is now: @{ Present; Value }.
function Get-Frequency {
    try {
        $item = Get-ItemProperty -LiteralPath $restoreKey -ErrorAction Stop
    }
    catch {
        return @{ Present = $false; Value = $null }
    }
    $prop = $item.PSObject.Properties[$frequencyName]
    if ($null -eq $prop) {
        return @{ Present = $false; Value = $null }
    }
    return @{ Present = $true; Value = $prop.Value }
}

$original = Get-Frequency
$lifted = $false
try {
    Set-ItemProperty -LiteralPath $restoreKey -Name $frequencyName -Value 0 -Type DWord -ErrorAction Stop
    $lifted = $true
}
catch {
    Write-Verbose ('could not lift the 24-hour limit: ' + $_.Exception.Message)
}

$before = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count
try {
    Checkpoint-Computer -Description $Description -RestorePointType 'MODIFY_SETTINGS' -WarningAction SilentlyContinue
}
finally {
    if ($lifted) {
        if ($original.Present) {
            Set-ItemProperty -LiteralPath $restoreKey -Name $frequencyName -Value $original.Value -Type DWord -ErrorAction SilentlyContinue
        }
        else {
            Remove-ItemProperty -LiteralPath $restoreKey -Name $frequencyName -ErrorAction SilentlyContinue
        }
    }
}
$after = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count

$result = 'skipped'
if ($after -gt $before) { $result = 'created' }

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ before = $before; after = $after; limit_lifted = $lifted }
}
