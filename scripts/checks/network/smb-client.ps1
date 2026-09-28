# Check: network.smb-client
# Did Windows refuse to open a shared folder lately because the other side is
# too old or insecure? Read-only; medkit never lowers these defenses.
# From the log Microsoft-Windows-SmbClient/Security, the last 30 days:
# - 32000: "SMB1 negotiate response received from remote device when SMB1
#   cannot be negotiated by the local computer" (Microsoft, "SMBv1 is not
#   installed by default"): an old NAS, router or Linux box that only speaks
#   SMB1. Microsoft strongly recommends not to reinstall SMB1; the device
#   needs an update or a setting for SMB2 / SMB3.
# - 31017: "Rejected an insecure guest logon" (Microsoft, "Enable insecure
#   guest logons in SMB2 and SMB3"): the share only allows guests, without a
#   password. Guests are refused by default on Windows 10 Enterprise /
#   Education, Windows 11 Pro and later; Windows 11 24H2 also requires SMB
#   signing, which guests cannot do. Microsoft recommends not to enable
#   guest logons; the share needs an account with a password.
# Also reported as facts, from
# HKLM\SYSTEM\CurrentControlSet\Services\LanmanWorkstation\Parameters and the
# policy key HKLM\SOFTWARE\Policies\Microsoft\Windows\LanmanWorkstation:
# signing_required (RequireSecuritySignature), guest_allowed
# (AllowInsecureGuestAuth, the policy wins); '' when not set.
# Result codes: smb1-device / guest-rejected / ok / unreadable (the log
# cannot be read). Facts: smb1_events, guest_events (counts; server names in
# the events are not output), signing_required, guest_allowed.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$logName = 'Microsoft-Windows-SmbClient/Security'
$days = 30
$parametersKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\LanmanWorkstation\Parameters'
$policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\LanmanWorkstation'

# A DWORD as true / false, or '' when the value is not there.
function Get-Flag {
    param([string]$Path, [string]$Name)
    try {
        $value = (Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop).$Name
        return ([string]$value -eq '1')
    }
    catch {
        return ''
    }
}

$facts = [ordered]@{ smb1_events = 0; guest_events = 0; signing_required = ''; guest_allowed = '' }
$facts.signing_required = Get-Flag $parametersKey 'RequireSecuritySignature'
$facts.guest_allowed = Get-Flag $policyKey 'AllowInsecureGuestAuth'
if ($facts.guest_allowed -is [string]) {
    $facts.guest_allowed = Get-Flag $parametersKey 'AllowInsecureGuestAuth'
}

$result = 'ok'
$events = @()
try {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = $logName; Id = @(32000, 31017); StartTime = (Get-Date).AddDays(-$days) } -MaxEvents 200 -ErrorAction Stop)
}
catch {
    # "No events were found" is not a failure; a missing or unreadable log is.
    if ([string]$_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') {
        $result = 'unreadable'
    }
}
if ($result -ne 'unreadable') {
    $facts.smb1_events = @($events | Where-Object { $_.Id -eq 32000 }).Count
    $facts.guest_events = @($events | Where-Object { $_.Id -eq 31017 }).Count
    if ($facts.smb1_events -gt 0) {
        $result = 'smb1-device'
    }
    elseif ($facts.guest_events -gt 0) {
        $result = 'guest-rejected'
    }
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
