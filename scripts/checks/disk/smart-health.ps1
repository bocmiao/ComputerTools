# Check: disk.smart-health
# Health of every physical disk (Get-PhysicalDisk HealthStatus / OperationalStatus),
# plus wear, temperature and power-on hours from Get-StorageReliabilityCounter when
# the disk exposes them. The worst disk decides the result.
# A disk whose health is Unknown is never counted as healthy. Internal disks are
# everything except USB disks, mounted virtual disks (File Backed Virtual) and
# SD / MMC cards other than the system disk (eMMC); when an internal disk is
# Unknown and no disk reports a problem, the result is "incomplete".
# Read-only. Needs administrator rights (reliability counters).
# Result codes: healthy / warning / unhealthy / incomplete. If no disk at all
# reports a known health status and none of them is internal, the script throws
# and the engine shows "unknown".

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# MSFT_PhysicalDisk value maps (Storage Management API, msft_physicaldisk.mof).
# Get-PhysicalDisk normally returns enum names; raw numbers are mapped here so
# that both forms are handled the same way.
$healthNames = @{ '0' = 'Healthy'; '1' = 'Warning'; '2' = 'Unhealthy'; '5' = 'Unknown' }
$opNames = @{
    '0' = 'Unknown'; '1' = 'Other'; '2' = 'OK'; '3' = 'Degraded'; '4' = 'Stressed'
    '5' = 'Predictive Failure'; '6' = 'Error'; '7' = 'Non-Recoverable Error'
    '8' = 'Starting'; '9' = 'Stopping'; '10' = 'Stopped'; '11' = 'In Service'
    '12' = 'No Contact'; '13' = 'Lost Communication'; '14' = 'Aborted'; '15' = 'Dormant'
    '16' = 'Supporting Entity in Error'; '17' = 'Completed'; '18' = 'Power Mode'
    '19' = 'Relocating'; '53252' = 'Failed Media'; '53253' = 'Split'
    '53254' = 'Stale Metadata'; '53255' = 'IO Error'; '53256' = 'Unrecognized Metadata'
}

# Operational states that mean the disk is failing or about to fail.
$unhealthyOps = @('Predictive Failure', 'Non-Recoverable Error', 'Failed Media')
# Operational states that deserve a warning.
$warningOps = @('Degraded', 'Stressed', 'Error', 'IO Error')
# Wear is "percentage of rated life used"; 100 means the rated limit is reached.
$wearWarningPct = 90

$rank = @{ 'unknown' = -1; 'healthy' = 0; 'warning' = 1; 'unhealthy' = 2 }

function ConvertTo-Name {
    param($Value, [hashtable]$Names)
    $text = [string]$Value
    if ($Names.ContainsKey($text)) {
        return $Names[$text]
    }
    return $text
}

$externalBuses = @('USB', '7', 'File Backed Virtual', '15')
$cardBuses = @('SD', '12', 'MMC', '13')

$disks = @(Get-PhysicalDisk)
if ($disks.Count -eq 0) {
    throw 'Get-PhysicalDisk returned no disks'
}

# The disk that holds Windows counts as internal even on an SD / MMC bus (eMMC).
$systemDisk = ''
try {
    $letter = ([string]$env:SystemDrive).TrimEnd(':')
    if ($letter -match '^[A-Za-z]$') {
        $systemDisk = [string](Get-Partition -DriveLetter $letter -ErrorAction Stop).DiskNumber
    }
}
catch {
    $systemDisk = ''
}

$worstLevel = 'unknown'
$worst = $null
$knownCount = 0
$unknownInternal = New-Object System.Collections.Generic.List[string]
$summaries = New-Object System.Collections.Generic.List[string]

foreach ($disk in $disks) {
    $name = ([string]$disk.FriendlyName).Trim()
    if ($name.Length -eq 0) {
        $name = 'Disk ' + [string]$disk.DeviceId
    }
    $bus = [string]$disk.BusType
    $internal = -not ($externalBuses -contains $bus)
    if ($internal -and ($cardBuses -contains $bus) -and ([string]$disk.DeviceId -ne $systemDisk)) {
        $internal = $false
    }

    $health = ConvertTo-Name -Value $disk.HealthStatus -Names $healthNames
    if ([string]::IsNullOrEmpty($health)) {
        $health = 'Unknown'
    }
    $ops = @()
    foreach ($op in @($disk.OperationalStatus)) {
        if ($null -ne $op) {
            $ops += (ConvertTo-Name -Value $op -Names $opNames)
        }
    }

    # Reliability counters are optional: many USB, RAID and virtual disks do not
    # expose them. A failure here only drops the extra facts; the result is still
    # decided by HealthStatus / OperationalStatus.
    $wear = $null
    $temperature = $null
    $hours = $null
    $counter = $null
    try {
        $counter = $disk | Get-StorageReliabilityCounter -ErrorAction Stop
    }
    catch {
        $counter = $null
    }
    if ($null -ne $counter) {
        if ($null -ne $counter.Wear) {
            $wear = [int]$counter.Wear
        }
        if (($null -ne $counter.Temperature) -and ([int]$counter.Temperature -gt 0)) {
            $temperature = [int]$counter.Temperature
        }
        if ($null -ne $counter.PowerOnHours) {
            $hours = [int]$counter.PowerOnHours
        }
    }

    $level = 'unknown'
    switch ($health) {
        'Healthy' { $level = 'healthy' }
        'Warning' { $level = 'warning' }
        'Unhealthy' { $level = 'unhealthy' }
    }
    foreach ($op in $ops) {
        if ($unhealthyOps -contains $op) {
            $level = 'unhealthy'
        }
        elseif (($warningOps -contains $op) -and ($rank[$level] -lt $rank['warning'])) {
            $level = 'warning'
        }
    }
    if (($null -ne $wear) -and ($wear -ge $wearWarningPct) -and ($rank[$level] -lt $rank['warning'])) {
        $level = 'warning'
    }

    $parts = New-Object System.Collections.Generic.List[string]
    $parts.Add($health)
    if ($ops.Count -gt 0) {
        $parts.Add(($ops -join '/'))
    }
    if ($null -ne $wear) {
        $parts.Add(('wear {0}%' -f $wear))
    }
    if ($null -ne $temperature) {
        $parts.Add(('{0}C' -f $temperature))
    }
    if ($null -ne $hours) {
        $parts.Add(('{0}h' -f $hours))
    }
    $summaries.Add(('{0}: {1}' -f $name, ($parts -join ', ')))

    if ($level -eq 'unknown') {
        if ($internal) {
            $unknownInternal.Add($name)
        }
        continue
    }
    $knownCount++
    if ($rank[$level] -gt $rank[$worstLevel]) {
        $worstLevel = $level
        $worst = [pscustomobject]@{
            Name        = $name
            Health      = $health
            Wear        = $wear
            Temperature = $temperature
            Hours       = $hours
        }
    }
}

$result = $worstLevel
if (($worstLevel -ne 'warning') -and ($worstLevel -ne 'unhealthy') -and ($unknownInternal.Count -gt 0)) {
    # No disk reports a problem, but at least one internal disk could not be checked.
    $result = 'incomplete'
}
elseif ($worstLevel -eq 'unknown') {
    throw ('No disk reported a known health status: {0}' -f ($summaries -join '; '))
}

$facts = [ordered]@{
    disk_count  = $disks.Count
    known_count = $knownCount
}
if ($null -ne $worst) {
    $facts['worst_disk'] = $worst.Name
    $facts['worst_health'] = $worst.Health
    if ($null -ne $worst.Wear) {
        $facts['wear_pct'] = $worst.Wear
    }
    if ($null -ne $worst.Temperature) {
        $facts['temperature_c'] = $worst.Temperature
    }
    if ($null -ne $worst.Hours) {
        $facts['power_on_hours'] = $worst.Hours
    }
}
$facts['unknown_count'] = $unknownInternal.Count
if ($unknownInternal.Count -gt 0) {
    $facts['unknown_disks'] = ($unknownInternal -join ', ')
}
$facts['disks'] = ($summaries -join '; ')

[pscustomobject]@{
    result = $result
    facts  = $facts
}
