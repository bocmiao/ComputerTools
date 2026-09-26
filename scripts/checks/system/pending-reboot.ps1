# Check: system.pending-reboot
# Is Windows waiting for a restart?
#   HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending (key)
#   HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired (key)
# PendingFileRenameOperations is reported as a fact only: many installers leave it
# behind and it does not mean an update is waiting.
# Read-only. Result codes: none / pending.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# True if the key exists, false if it does not. Any other error (for example
# access denied) is not swallowed.
function Test-RegistryKey {
    param([string]$Path)
    try {
        $null = Get-Item -LiteralPath $Path -ErrorAction Stop
        return $true
    }
    catch [System.Management.Automation.ItemNotFoundException] {
        return $false
    }
}

$cbs = Test-RegistryKey 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending'
$wu = Test-RegistryKey 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired'

$renamePending = $false
$sessionManager = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager'
if ($null -ne $sessionManager) {
    $prop = $sessionManager.PSObject.Properties['PendingFileRenameOperations']
    if (($null -ne $prop) -and ($null -ne $prop.Value)) {
        $entries = @($prop.Value | Where-Object { -not [string]::IsNullOrEmpty([string]$_) })
        $renamePending = ($entries.Count -gt 0)
    }
}

$result = 'none'
if ($cbs -or $wu) {
    $result = 'pending'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        cbs_reboot_pending     = $cbs
        update_reboot_required = $wu
        file_rename_pending    = $renamePending
    }
}
