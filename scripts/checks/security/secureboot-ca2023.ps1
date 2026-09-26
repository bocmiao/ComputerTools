# Check: security.secureboot-ca2023
# Has the Secure Boot signature database (db) received the "Windows UEFI CA 2023"
# certificate? Microsoft's 2011 Secure Boot certificates expire during 2026
# (Windows Production PCA 2011 on 2026-10-19).
#   1. Confirm-SecureBootUEFI: True = on, False = off (UEFI), throws
#      PlatformNotSupportedException ("Cmdlet not supported on this platform",
#      0xC0000002) on legacy BIOS or firmware without Secure Boot.
#   2. When Secure Boot is on: Get-SecureBootUEFI -Name db, decode the bytes as
#      ASCII and look for 'Windows UEFI CA 2023' (the check Microsoft publishes).
# The servicing status under HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\Servicing
# (UEFICA2023Status, WindowsUEFICA2023Capable) is informational only and is
# reported as facts when present.
# Read-only. Needs administrator rights ("Access was denied" is thrown otherwise).
# Result codes: updated / not-updated / secureboot-off / legacy.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Test-PlatformNotSupported {
    param($ErrorRecord)
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        if ($ex -is [System.PlatformNotSupportedException]) {
            return $true
        }
        $ex = $ex.InnerException
    }
    # 0xC0000002 = STATUS_NOT_IMPLEMENTED. The message text itself is localized,
    # the status code is not.
    return ([string]$ErrorRecord.Exception.Message -match '0xC0000002')
}

$uefi = $true
$secureBoot = $false
try {
    $secureBoot = [bool](Confirm-SecureBootUEFI)
}
catch {
    if (Test-PlatformNotSupported $_) {
        $uefi = $false
    }
    else {
        throw
    }
}

$facts = [ordered]@{
    uefi        = $uefi
    secure_boot = $secureBoot
}

$servicingPath = 'HKLM:\SYSTEM\CurrentControlSet\Control\SecureBoot\Servicing'
if (Test-Path -LiteralPath $servicingPath) {
    $servicing = Get-ItemProperty -LiteralPath $servicingPath
    if ($null -ne $servicing) {
        $status = $servicing.PSObject.Properties['UEFICA2023Status']
        if (($null -ne $status) -and ($null -ne $status.Value)) {
            $facts['servicing_status'] = [string]$status.Value
        }
        $capable = $servicing.PSObject.Properties['WindowsUEFICA2023Capable']
        if (($null -ne $capable) -and ($null -ne $capable.Value)) {
            $facts['ca2023_capable'] = [int]$capable.Value
        }
    }
}

if (-not $uefi) {
    $result = 'legacy'
}
elseif (-not $secureBoot) {
    $result = 'secureboot-off'
}
else {
    $db = Get-SecureBootUEFI -Name db
    $text = [System.Text.Encoding]::ASCII.GetString([byte[]]$db.Bytes)
    $hasCa2023 = $text.Contains('Windows UEFI CA 2023')
    $facts['ca2023_in_db'] = $hasCa2023
    if ($hasCa2023) {
        $result = 'updated'
    }
    else {
        $result = 'not-updated'
    }
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
