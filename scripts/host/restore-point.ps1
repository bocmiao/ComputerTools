#Requires -Version 5.1
<#
.SYNOPSIS
    Create a system restore point before a caution/danger level change.

.DESCRIPTION
    Windows creates at most one restore point per 24 hours through this API by
    default; in that case Checkpoint-Computer only writes a warning. We compare
    the number of restore points before and after to tell "created" from
    "skipped". Throws when System Restore is turned off.

    Result codes: created, skipped.
#>
[CmdletBinding()]
param(
    [string]$Description = 'medkit'
)

$ErrorActionPreference = 'Stop'

$before = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count
Checkpoint-Computer -Description $Description -RestorePointType 'MODIFY_SETTINGS' -WarningAction SilentlyContinue
$after = @(Get-ComputerRestorePoint -ErrorAction SilentlyContinue).Count

$result = 'skipped'
if ($after -gt $before) { $result = 'created' }

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ before = $before; after = $after }
}
