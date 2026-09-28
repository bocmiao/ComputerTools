# Check: system.maintenance-wake
# May Windows wake the PC for Automatic Maintenance? Control Panel > Security
# and Maintenance > Maintenance > "Change maintenance settings" > "Allow
# scheduled maintenance to wake up my computer at the scheduled time" is the
# DWORD WakeUp under HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\
# Schedule\Maintenance: 1 allows it, 0 does not. Without the value the
# maintenance scheduler wakes the PC (the "enable" of Win10-Initial-Setup-
# Script, MIT, removes the value; Microsoft: the PC "automatically is resumed
# from sleep ... respecting the Power Management policy"). The Group Policy
# "Automatic Maintenance WakeUp Policy" (WakeUp under HKLM\SOFTWARE\Policies\
# Microsoft\Windows\Task Scheduler\Maintenance) wins over it; it is only
# reported. Waking also needs wake timers to be allowed (system.wake-timers).
# Read-only.
# Result codes: policy-on (manual) / policy-off / on (advice; fix
# power.maintenance-wake-off) / off.
# Facts: wake_up, policy (the values, -1 when missing).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$settingKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance'
$policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\Task Scheduler\Maintenance'

# A DWORD value, or -1 when the key or the value is not there (or it is not a
# DWORD).
function Get-Dword {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop
    }
    catch {
        return -1
    }
    $value = $item.$Name
    if ($value -is [int]) {
        return $value
    }
    return -1
}

$wakeUp = Get-Dword $settingKey 'WakeUp'
$policy = Get-Dword $policyKey 'WakeUp'

$result = 'on'
if ($policy -eq 0) {
    $result = 'policy-off'
}
elseif ($policy -gt 0) {
    $result = 'policy-on'
}
elseif ($wakeUp -eq 0) {
    $result = 'off'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        wake_up = $wakeUp
        policy  = $policy
    }
}
