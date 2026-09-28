# Check: disk.smart-health
# Health of every physical disk (Get-PhysicalDisk HealthStatus / OperationalStatus),
# plus wear, temperature and power-on hours from Get-StorageReliabilityCounter when
# the disk exposes them, and the bad sector counts of hard disks (see
# Get-SmartSectors). The worst disk decides the result.
# A disk whose health is Unknown is never counted as healthy. Internal disks are
# everything except USB disks, mounted virtual disks (File Backed Virtual) and
# SD / MMC cards other than the system disk (eMMC); when an internal disk is
# Unknown and no disk reports a problem, the result is "incomplete".
# Read-only. Needs administrator rights (reliability counters).
# Result codes: healthy / warning / unhealthy / incomplete. If no disk at all
# reports a known health status and none of them is internal, the script throws
# and the engine shows "unknown".
# Facts of the worst disk also: sign (bad-sectors: a hard disk with bad
# sectors; wear: an SSD past $wearWarningPct percent of its rated life),
# reallocated_sectors, pending_sectors, uncorrectable_sectors (hard disks whose
# SMART data could be read).

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

# The bad sector counts of the disks' ATA SMART data, by disk number:
# @{ Reallocated; Pending; Uncorrectable } ($null when not reported).
# MSStorageDriver_FailurePredictData (root\wmi) holds the SMART data the disk
# driver read: VendorSpecific has 30 entries of 12 bytes from offset 2 (id,
# flags (2 bytes), value, worst, raw value (6 bytes), reserved); the lower 4
# bytes of the raw value are the count. 05 is Reallocated Sectors Count, C5
# Current Pending Sector Count, C6 Offline Uncorrectable. Its InstanceName is
# the disk's PNPDeviceID followed by "_0"; Win32_DiskDrive.Index is the disk
# number (Get-PhysicalDisk DeviceId). NVMe, USB and RAID disks usually have no
# entry (the query can also fail with "Not supported"): then nothing is added.
function Get-SmartSectors {
    $byDisk = @{}
    try {
        $data = @(Get-CimInstance -Namespace 'root\wmi' -ClassName 'MSStorageDriver_FailurePredictData' -ErrorAction Stop)
        $drives = @(Get-CimInstance -ClassName 'Win32_DiskDrive' -Property 'Index', 'PNPDeviceID' -ErrorAction Stop)
    }
    catch {
        Write-Verbose ('no SMART data: ' + $_.Exception.Message)
        return $byDisk
    }
    $numbers = @{}
    foreach ($drive in $drives) {
        $pnp = ([string]$drive.PNPDeviceID).ToUpperInvariant()
        if ($pnp.Length -gt 0) {
            $numbers[$pnp] = [string]$drive.Index
        }
    }
    foreach ($entry in $data) {
        $instance = ([string]$entry.InstanceName).ToUpperInvariant() -replace '_\d+$', ''
        if (-not $numbers.ContainsKey($instance)) {
            continue
        }
        $bytes = [byte[]]@($entry.VendorSpecific)
        $counts = @{ Reallocated = $null; Pending = $null; Uncorrectable = $null }
        for ($i = 0; $i -lt 30; $i++) {
            $offset = 2 + ($i * 12)
            if (($offset + 11) -gt $bytes.Length) {
                break
            }
            $raw = [int64][BitConverter]::ToUInt32($bytes, $offset + 5)
            switch ([int]$bytes[$offset]) {
                0x05 { $counts.Reallocated = $raw }
                0xC5 { $counts.Pending = $raw }
                0xC6 { $counts.Uncorrectable = $raw }
            }
        }
        if (($null -ne $counts.Reallocated) -or ($null -ne $counts.Pending) -or ($null -ne $counts.Uncorrectable)) {
            $byDisk[$numbers[$instance]] = $counts
        }
    }
    return $byDisk
}

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

$sectors = Get-SmartSectors

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
    $sign = ''
    if (($null -ne $wear) -and ($wear -ge $wearWarningPct)) {
        $sign = 'wear'
        if ($rank[$level] -lt $rank['warning']) {
            $level = 'warning'
        }
    }

    # Bad sectors count on hard disks only: SSDs use these IDs for other
    # counters.
    $counts = $null
    $isHdd = @('HDD', '3') -contains [string]$disk.MediaType
    if ($isHdd -and $sectors.ContainsKey([string]$disk.DeviceId)) {
        $counts = $sectors[[string]$disk.DeviceId]
        $bad = @($counts.Reallocated, $counts.Pending, $counts.Uncorrectable | Where-Object { ($null -ne $_) -and ($_ -gt 0) })
        if ($bad.Count -gt 0) {
            $sign = 'bad-sectors'
            if ($rank[$level] -lt $rank['warning']) {
                $level = 'warning'
            }
        }
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
    if ($null -ne $counts) {
        $parts.Add(('05/C5/C6 {0}/{1}/{2}' -f $counts.Reallocated, $counts.Pending, $counts.Uncorrectable))
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
            Sign        = $sign
            Sectors     = $counts
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
    if ($worst.Sign.Length -gt 0) {
        $facts['sign'] = $worst.Sign
    }
    if ($null -ne $worst.Sectors) {
        foreach ($pair in @(@('reallocated_sectors', 'Reallocated'), @('pending_sectors', 'Pending'), @('uncorrectable_sectors', 'Uncorrectable'))) {
            if ($null -ne $worst.Sectors[$pair[1]]) {
                $facts[$pair[0]] = $worst.Sectors[$pair[1]]
            }
        }
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
