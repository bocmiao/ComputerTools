# Check: disk.error-events
# Counts disk error events in the System log over the last 30 days:
#   source "disk": 7 (bad block), 51 (error during paging), 153 (IO retried)
#   source "Ntfs": 55 (file system corruption)
# Read-only. Result codes: none / found.
#
# The query filters by log, ID and time only; the provider is matched here in
# PowerShell. Passing ProviderName to -FilterHashtable makes Get-WinEvent look up
# provider metadata first, which fails on some systems (NoMatchingProvidersFound,
# LogsAndProvidersDontOverlap). The provider match below is required anyway:
# ID 55 is also logged by Kernel-Processor-Power at every boot.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 30
$since = (Get-Date).AddDays(-$days)

$events = @()
try {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; Id = 7, 51, 55, 153; StartTime = $since } -ErrorAction Stop)
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

$badBlock = 0
$paging = 0
$retry = 0
$ntfs = 0
$last = $null
$diskNumbers = New-Object System.Collections.Generic.List[string]

foreach ($e in $events) {
    $provider = [string]$e.ProviderName
    $id = [int]$e.Id
    $hit = $false

    if ($provider -eq 'disk') {
        if ($id -eq 7) {
            $badBlock++
            $hit = $true
        }
        elseif ($id -eq 51) {
            $paging++
            $hit = $true
        }
        elseif ($id -eq 153) {
            $retry++
            $hit = $true
        }
        if ($hit) {
            # The device path (\Device\HarddiskN\DRN) is one of the insertion strings.
            foreach ($p in @($e.Properties)) {
                $m = [regex]::Match([string]$p.Value, 'Harddisk(\d+)')
                if ($m.Success) {
                    if (-not $diskNumbers.Contains($m.Groups[1].Value)) {
                        $diskNumbers.Add($m.Groups[1].Value)
                    }
                    break
                }
            }
        }
    }
    elseif ((($provider -eq 'Ntfs') -or ($provider -eq 'Microsoft-Windows-Ntfs')) -and ($id -eq 55)) {
        $ntfs++
        $hit = $true
    }

    if ($hit -and (($null -eq $last) -or ($e.TimeCreated -gt $last))) {
        $last = $e.TimeCreated
    }
}

$total = $badBlock + $paging + $retry + $ntfs

$facts = [ordered]@{
    days         = $days
    total        = $total
    bad_block    = $badBlock
    paging_error = $paging
    io_retry     = $retry
    ntfs_corrupt = $ntfs
}
if ($diskNumbers.Count -gt 0) {
    $facts['disk_numbers'] = ($diskNumbers -join ', ')
}
if ($null -ne $last) {
    $facts['last_time'] = $last.ToString('o')
}

$result = 'none'
if ($total -gt 0) {
    $result = 'found'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
