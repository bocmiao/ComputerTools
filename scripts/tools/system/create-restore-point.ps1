# Tool: system.create-restore-point (action)
# Makes a system restore point now, like the "Create" button in System
# Protection (Checkpoint-Computer), named "xiao yao xiang shou dong chuang jian"
# (made by hand with medkit; built from character codes, the script stays
# ASCII). Windows skips a second restore point within 24 hours; as in
# host/restore-point.ps1, that limit is lifted for this one call with
# Microsoft's documented switch (SystemRestorePointCreationFrequency = 0) and
# put back right after (removed when it was not there).
# No setting is changed (the switch is put back), so this is a tool.
# Result codes: created (ok) / skipped (advice: Windows made none) / failed
# (advice: most often System Protection is off for the system drive) /
# unsupported (na: no System Restore here, like Windows Server).
# Facts: before, after (the number of restore points), error (failed only:
# the error text).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$restoreKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'
$frequencyName = 'SystemRestorePointCreationFrequency'
$description = -join [char[]](0x5C0F, 0x836F, 0x7BB1, 0x624B, 0x52A8, 0x521B, 0x5EFA)

# Windows Server has no System Restore (ProductType 1 is a workstation).
$productType = 1
try {
    $productType = [int](Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop).ProductType
}
catch {
    Write-Verbose ('could not read the product type: ' + $_.Exception.Message)
}
if (($productType -ne 1) -or
    ($null -eq (Get-Command -Name 'Checkpoint-Computer' -ErrorAction SilentlyContinue)) -or
    ($null -eq (Get-Command -Name 'Get-ComputerRestorePoint' -ErrorAction SilentlyContinue))) {
    [pscustomobject]@{ result = 'unsupported'; facts = [ordered]@{} }
    return
}

# The value as it is now: @{ Present; Value }.
function Get-Frequency {
    try {
        $item = Get-ItemProperty -LiteralPath $restoreKey -ErrorAction Stop
    }
    catch {
        return @{ Present = $false; Value = $null }
    }
    if ($null -eq $item) {
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
$failure = ''
try {
    Checkpoint-Computer -Description $description -RestorePointType 'MODIFY_SETTINGS' -WarningAction SilentlyContinue
}
catch {
    $failure = ([string]$_.Exception.Message).Trim()
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

$facts = [ordered]@{ before = $before; after = $after }
$result = 'skipped'
if ($failure.Length -gt 0) {
    $result = 'failed'
    $facts['error'] = $failure
}
elseif ($after -gt $before) {
    $result = 'created'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
