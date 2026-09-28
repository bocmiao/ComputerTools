# Feature: explorer.delete-confirm -- break (tests only)
# Sets the fNoConfirmRecycle bit of ShellState: deleting to the Recycle Bin
# no longer asks first (the Windows default).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
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

if (-not (Set-NoConfirm $true)) {
    throw 'ShellState is missing'
}

[pscustomobject]@{ result = 'broken' }
