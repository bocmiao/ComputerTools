# Check: disk.error-events
# Disk and file system error events in the System log over the last 30 days:
#   source "disk": 7 (bad block), 51 (error during a paging operation),
#                  153 (IO request timed out and was retried)
#   source "Ntfs" / "Microsoft-Windows-Ntfs": 55 (file system structure corrupt),
#                  98 at level Error / Warning (volume needs a full chkdsk);
#                  98 is also logged at level Information at every boot
#                  ("volume is healthy"), which is not counted
# Microsoft describes 153 as a storage request that timed out (drivers, firmware,
# load), not as a bad sector, so a few 153 events on their own get their own,
# milder result.
# Events on removable disks are left out as far as they can be told apart: disks
# on the USB bus, and disks or drive letters that are not attached any more
# (unplugged USB sticks, SD cards). They are counted in excluded_removable.
# The query filters by log, ID and time only; the provider is matched here in
# PowerShell. Passing ProviderName to -FilterHashtable makes Get-WinEvent look up
# provider metadata first, which fails on some systems (NoMatchingProvidersFound,
# LogsAndProvidersDontOverlap). The provider match below is required anyway:
# ID 55 is also logged by Kernel-Processor-Power at every boot.
# Read-only. Result codes:
#   none        nothing found
#   retries-few 1 to 4 153 events: normal noise
#   retries     5 to 19 153 events and nothing else
#   fs-corrupt  file system corruption (55 / 98), no disk-level errors
#   found       7 or 51, or 20 or more 153 events

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 30
$manyRetries = 20
# Fewer than this many 153 events in 30 days are normal noise (retries-few, status ok).
$someRetries = 5
$since = (Get-Date).AddDays(-$days)

$events = @()
try {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; Id = 7, 51, 55, 98, 153; StartTime = $since } -ErrorAction Stop)
}
catch {
    # Get-WinEvent reports "no events" as an error; that simply means zero.
    if ([string]$_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {
        $events = @()
    }
    else {
        throw
    }
}

# Disks and drive letters attached right now, to leave out removable disks.
# Filled on first use. If the storage cmdlets are not available nothing is left out.
$script:diskBus = $null
$script:letterDisk = $null
function Initialize-DiskMap {
    if ($null -ne $script:diskBus) {
        return
    }
    $script:diskBus = @{}
    $script:letterDisk = @{}
    try {
        foreach ($d in @(Get-Disk -ErrorAction Stop)) {
            $script:diskBus[[string]$d.Number] = [string]$d.BusType
        }
        foreach ($p in @(Get-Partition -ErrorAction Stop)) {
            $letter = [string]$p.DriveLetter
            if ($letter -match '^[A-Za-z]$') {
                $script:letterDisk[$letter.ToUpperInvariant()] = [string]$p.DiskNumber
            }
        }
    }
    catch {
        $script:diskBus = @{}
        $script:letterDisk = $null
    }
}

# True when the disk is on the USB bus or is not attached any more.
function Test-RemovableDisk {
    param([string]$Number)
    Initialize-DiskMap
    if ($null -eq $script:letterDisk) {
        return $false
    }
    if (-not $script:diskBus.ContainsKey($Number)) {
        return $true
    }
    $bus = $script:diskBus[$Number]
    return (($bus -eq 'USB') -or ($bus -eq '7'))
}

function Test-RemovableLetter {
    param([string]$Letter)
    Initialize-DiskMap
    if ($null -eq $script:letterDisk) {
        return $false
    }
    $key = $Letter.ToUpperInvariant()
    if (-not $script:letterDisk.ContainsKey($key)) {
        return $true
    }
    return (Test-RemovableDisk $script:letterDisk[$key])
}

$badBlock = 0
$paging = 0
$retry = 0
$ntfs = 0
$excluded = 0
$last = $null
$diskNumbers = New-Object System.Collections.Generic.List[string]

foreach ($e in $events) {
    $provider = [string]$e.ProviderName
    $id = [int]$e.Id
    $kind = ''

    if (($provider -eq 'disk') -and (@(7, 51, 153) -contains $id)) {
        # The device path (\Device\HarddiskN\DRN) is one of the insertion strings.
        $number = ''
        foreach ($p in @($e.Properties)) {
            $m = [regex]::Match([string]$p.Value, 'Harddisk(\d+)')
            if ($m.Success) {
                $number = $m.Groups[1].Value
                break
            }
        }
        if (($number.Length -gt 0) -and (Test-RemovableDisk $number)) {
            $excluded++
            continue
        }
        if (($number.Length -gt 0) -and (-not $diskNumbers.Contains($number))) {
            $diskNumbers.Add($number)
        }
        if ($id -eq 7) {
            $badBlock++
        }
        elseif ($id -eq 51) {
            $paging++
        }
        else {
            $retry++
        }
        $kind = 'disk'
    }
    elseif ((($provider -eq 'Ntfs') -or ($provider -eq 'Microsoft-Windows-Ntfs')) -and (($id -eq 55) -or ($id -eq 98))) {
        # Level 1 = critical, 2 = error, 3 = warning; 98 at level 4 means "healthy".
        $level = [int]$e.Level
        if (($level -lt 1) -or ($level -gt 3)) {
            continue
        }
        $letter = ''
        foreach ($p in @($e.Properties)) {
            $m = [regex]::Match([string]$p.Value, '^\s*([A-Za-z]):\\?\s*$')
            if ($m.Success) {
                $letter = $m.Groups[1].Value
                break
            }
        }
        if (($letter.Length -gt 0) -and (Test-RemovableLetter $letter)) {
            $excluded++
            continue
        }
        $ntfs++
        $kind = 'ntfs'
    }

    if (($kind.Length -gt 0) -and (($null -eq $last) -or ($e.TimeCreated -gt $last))) {
        $last = $e.TimeCreated
    }
}

$total = $badBlock + $paging + $retry + $ntfs

$facts = [ordered]@{
    days               = $days
    total              = $total
    bad_block          = $badBlock
    paging_error       = $paging
    io_retry           = $retry
    ntfs_corrupt       = $ntfs
    excluded_removable = $excluded
}
if ($diskNumbers.Count -gt 0) {
    $facts['disk_numbers'] = ($diskNumbers -join ', ')
}
if ($null -ne $last) {
    $facts['last_time'] = $last.ToString('o')
}

$result = 'none'
if ((($badBlock + $paging) -gt 0) -or ($retry -ge $manyRetries)) {
    $result = 'found'
}
elseif ($ntfs -gt 0) {
    $result = 'fs-corrupt'
}
elseif ($retry -ge $someRetries) {
    $result = 'retries'
}
elseif ($retry -gt 0) {
    $result = 'retries-few'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
