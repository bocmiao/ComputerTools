# Check: update.restart-settings
# Does Windows warn before it restarts to finish updates, and does it keep
# out of the hours the PC is used? Read-only. From the values the Windows
# Update page of Settings keeps under
# HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings (not policies):
#   RestartNotificationsAllowed2 = 1   "Notify me when a restart is required
#                                      to finish updating" (fix:
#                                      update.restart-notify)
#   SmartActiveHoursState = 1          active hours follow how the PC is used
#                                      (fix: update.smart-active-hours);
#                                      otherwise ActiveHoursStart / End (the
#                                      hours, default 8 to 17)
# Result codes: ok (both on) / notify-off / hours-off / both-off.
# Facts: notify, smart_hours (true / false), hours_start, hours_end (the
# manual active hours, 0 to 23).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$uxKey = 'HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings'

function Get-Number {
    param($Item, [string]$Name, [int]$Default)
    if ($null -eq $Item) {
        return $Default
    }
    $property = $Item.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $Default
    }
    try {
        return [int]$property.Value
    }
    catch {
        return $Default
    }
}

$item = $null
try {
    $item = Get-ItemProperty -LiteralPath $uxKey -ErrorAction Stop
}
catch {
    Write-Verbose 'no Windows Update settings yet'
}
$notify = ((Get-Number $item 'RestartNotificationsAllowed2' 0) -eq 1)
$smart = ((Get-Number $item 'SmartActiveHoursState' 0) -eq 1)
$start = Get-Number $item 'ActiveHoursStart' 8
$end = Get-Number $item 'ActiveHoursEnd' 17

$result = 'ok'
if ((-not $notify) -and (-not $smart)) {
    $result = 'both-off'
}
elseif (-not $notify) {
    $result = 'notify-off'
}
elseif (-not $smart) {
    $result = 'hours-off'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ notify = $notify; smart_hours = $smart; hours_start = $start; hours_end = $end }
}
