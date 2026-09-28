# Check: hardware.battery-charging
# Is the laptop charging while the charger is plugged in?
#   No Win32_Battery instance -> no-battery (desktop PC; status na).
#   root\wmi BatteryStatus (one instance per battery, from the battery driver):
#   PowerOnline (the charger is connected), Charging, Discharging, RemainingCapacity (mWh);
#   root\wmi BatteryFullChargedCapacity: FullChargedCapacity (mWh).
#   The charge level is the remaining capacity of all batteries over their full-charge
#   capacity, like the taskbar shows it; when a battery does not report its capacities,
#   Win32_Battery.EstimatedChargeRemaining is used instead.
# Result codes:
#   on-battery    running on the battery (the charger is not connected, or not detected)
#   charging      plugged in and charging
#   full          plugged in, not charging, 95 % or more (the battery tops up only after
#                 it has run down a little, so this is normal)
#   not-charging  plugged in, not charging, below 95 % (the taskbar says "plugged in, not
#                 charging"; usually a vendor charge limit that stops at 60 % or 80 %)
#   draining      plugged in but still discharging (the charger is too weak for the PC)
#   no-battery    no battery
# Read-only.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# The battery driver reports this when it does not know a capacity.
$unknownCapacity = [uint32]::MaxValue

function Get-KnownCapacity {
    param($Value)
    if ($null -eq $Value) {
        return $null
    }
    $number = [double]$Value
    if ($number -le 0 -or $number -ge $unknownCapacity) {
        return $null
    }
    return $number
}

$batteries = @(Get-CimInstance -ClassName Win32_Battery)
if ($batteries.Count -eq 0) {
    [pscustomobject]@{
        result = 'no-battery'
        facts  = [ordered]@{ battery_count = 0 }
    }
    return
}

$states = @(Get-CimInstance -Namespace 'root\wmi' -ClassName BatteryStatus)
if ($states.Count -eq 0) {
    throw 'The battery driver does not report the charging state'
}

$online = $false
$charging = $false
$discharging = $false
$remaining = [double]0
$remainingKnown = $true
foreach ($s in $states) {
    if ($s.PowerOnline) {
        $online = $true
    }
    if ($s.Charging) {
        $charging = $true
    }
    if ($s.Discharging) {
        $discharging = $true
    }
    $value = Get-KnownCapacity $s.RemainingCapacity
    if ($null -eq $value) {
        # 0 mWh left is a real reading: an empty or dead battery.
        if ($null -ne $s.RemainingCapacity -and [double]$s.RemainingCapacity -eq 0) {
            continue
        }
        $remainingKnown = $false
    }
    else {
        $remaining += $value
    }
}

$full = [double]0
$fullKnown = $true
$capacities = @(Get-CimInstance -Namespace 'root\wmi' -ClassName BatteryFullChargedCapacity -ErrorAction SilentlyContinue)
if ($capacities.Count -eq 0) {
    $fullKnown = $false
}
foreach ($c in $capacities) {
    $value = Get-KnownCapacity $c.FullChargedCapacity
    if ($null -eq $value) {
        $fullKnown = $false
    }
    else {
        $full += $value
    }
}

$percent = $null
if ($remainingKnown -and $fullKnown -and $full -gt 0) {
    $percent = $remaining / $full * 100
}
else {
    $levels = @($batteries | ForEach-Object { $_.EstimatedChargeRemaining } | Where-Object { $null -ne $_ -and [double]$_ -ge 0 -and [double]$_ -le 100 })
    if ($levels.Count -gt 0) {
        $percent = ($levels | Measure-Object -Average).Average
    }
}
if ($null -eq $percent) {
    throw 'The battery does not report its charge level'
}
$percent = [int][math]::Round([math]::Min([math]::Max([double]$percent, 0), 100))

$result = 'on-battery'
if ($online) {
    if ($charging) {
        $result = 'charging'
    }
    elseif ($discharging) {
        $result = 'draining'
    }
    elseif ($percent -ge 95) {
        $result = 'full'
    }
    else {
        $result = 'not-charging'
    }
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        battery_count = $batteries.Count
        charge_pct    = $percent
        power_online  = $online
        charging      = $charging
        discharging   = $discharging
    }
}
