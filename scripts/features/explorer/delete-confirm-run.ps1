# Feature: explorer.delete-confirm -- run (and prepare)
# Makes deleting to the Recycle Bin ask first: clears the fNoConfirmRecycle
# bit of ShellState (see the shared block), the same as ticking "Display
# delete confirmation dialog" in the Recycle Bin properties. Explorer reads it
# when it starts, so Explorer is restarted afterwards (reboot: explorer).
# -Prepare: returns before = { exists, no_confirm }.
# Run: -Before is that JSON. Returns skipped (nothing changed) when the state
#   changed since, when there is no ShellState, or when it already asks.
#   Otherwise clears the bit and reads it back; when that fails, the bit is
#   set again and the script throws.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:',
    [bool]$Prepare = $false,
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block shell-state: identical in features/explorer/delete-confirm-detect.ps1, delete-confirm-run.ps1, delete-confirm-undo.ps1 and delete-confirm-break.ps1 (medkit-data check compares them) ----
# SHELLSTATE (Microsoft, "SHELLSTATEA structure") is kept by Explorer as the
# binary value ShellState under the user's Software\Microsoft\Windows\
# CurrentVersion\Explorer. Its fNoConfirmRecycle bit (byte 4, 0x04: the bit
# RecycleBinDeleteConfirmation in Sophia Script for Windows flips) is set when
# deleting to the Recycle Bin does not ask first. Only that bit is ever
# changed; every other byte is written back as it is at that moment.
$explorerKey = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Explorer'
$noConfirmByte = 4
$noConfirmBit = 0x04

# The ShellState bytes, or $null when the value is missing, not binary or too
# short.
function Get-ShellState {
    try {
        $item = Get-ItemProperty -LiteralPath $explorerKey -Name 'ShellState' -ErrorAction Stop
    }
    catch {
        return $null
    }
    $bytes = $item.ShellState
    if (($bytes -isnot [byte[]]) -or ($bytes.Length -le $noConfirmByte)) {
        return $null
    }
    return , $bytes
}

# True when the bytes say deleting does not ask first.
function Test-NoConfirm {
    param([byte[]]$Bytes)
    return (($Bytes[$noConfirmByte] -band $noConfirmBit) -ne 0)
}

# Sets ($true) or clears ($false) the fNoConfirmRecycle bit. Returns $false
# when there is no usable ShellState to change.
function Set-NoConfirm {
    param([bool]$On)
    $bytes = Get-ShellState
    if ($null -eq $bytes) {
        return $false
    }
    if ($On) {
        $bytes[$noConfirmByte] = [byte]($bytes[$noConfirmByte] -bor $noConfirmBit)
    }
    else {
        $bytes[$noConfirmByte] = [byte]($bytes[$noConfirmByte] -band (-bnot $noConfirmBit))
    }
    Set-ItemProperty -LiteralPath $explorerKey -Name 'ShellState' -Value $bytes -Type Binary -ErrorAction Stop
    return $true
}
# ---- end of shared block shell-state ----

$bytes = Get-ShellState
$exists = ($null -ne $bytes)
$noConfirm = $exists -and (Test-NoConfirm $bytes)
$snapshot = [ordered]@{ exists = $exists; no_confirm = $noConfirm }
if ($Prepare) {
    return [pscustomobject]@{ before = $snapshot }
}

$recorded = ConvertFrom-Json -InputObject $Before
if (([bool]$recorded.exists -ne $exists) -or ([bool]$recorded.no_confirm -ne $noConfirm)) {
    return [pscustomobject]@{ skipped = $true }
}
if (-not $noConfirm) {
    return [pscustomobject]@{ skipped = $true }
}

try {
    if (-not (Set-NoConfirm $false)) {
        throw 'ShellState is missing'
    }
    $after = Get-ShellState
    if (($null -eq $after) -or (Test-NoConfirm $after)) {
        throw 'ShellState did not change'
    }
}
catch {
    $failure = $_
    # Put the bit back (the engine runs the undo script as well).
    try {
        [void](Set-NoConfirm $true)
    }
    catch {
        Write-Verbose ('could not set the bit back: ' + $_.Exception.Message)
    }
    throw $failure
}

[pscustomobject]@{ after = [ordered]@{ no_confirm = $false } }
