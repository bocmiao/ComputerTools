# Check: update.history
# Did the latest updates install? Reads the update history that Settings shows
# (Windows Update Agent: Microsoft.Update.Session, QueryHistory). Read-only;
# nothing is searched, downloaded or installed, and no network is needed.
# An installation that failed in the last $days days counts, unless the same
# update installed later: the same KB number, or the same title once the
# numbers are left out (so that a later cumulative update, driver or Defender
# update of the same kind counts too).
# The error code decides the result code (the texts live in the YAML):
#   failed-space     not enough disk space
#   failed-network   could not reach Windows Update (network, proxy, time)
#   failed-files     damaged or missing update files (reset the components)
#   failed-restart   a restart is needed first
#   failed-upgrade   a new version of Windows could not be installed
#   failed-other     anything else
#   ok               nothing failed
#   empty            no history at all (new Windows, or the history was reset)
#   off              the Windows Update service is disabled
# Facts: failed_title, error_code ("0x80070643"), failed_date (yyyy-MM-dd),
# failed_count (failed updates not installed later), last_install (date of the
# latest successful installation, or '').

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 30
$maxEntries = 300
# Error codes (as Windows shows them) -> result code.
$codes = @{
    '0x80070070' = 'failed-space'
    '0x80070027' = 'failed-space'
    '0x8024402C' = 'failed-network'
    '0x8024401C' = 'failed-network'
    '0x80244022' = 'failed-network'
    '0x80072EE2' = 'failed-network'
    '0x80072EFD' = 'failed-network'
    '0x80072EFE' = 'failed-network'
    '0x80072F8F' = 'failed-network'
    '0x80D02002' = 'failed-network'
    '0x80070002' = 'failed-files'
    '0x80070003' = 'failed-files'
    '0x8007000D' = 'failed-files'
    '0x80073712' = 'failed-files'
    '0x800F081F' = 'failed-files'
    '0x800F0831' = 'failed-files'
    '0x80240034' = 'failed-files'
    '0x80246017' = 'failed-files'
    '0x80070020' = 'failed-restart'
    '0xC1900107' = 'failed-restart'
    '0xC1900101' = 'failed-upgrade'
    '0xC1900200' = 'failed-upgrade'
    '0xC1900208' = 'failed-upgrade'
}

# An HRESULT as Windows shows it: "0x80070643". HRESULTs usually come as
# signed 32-bit numbers (0x80070643 is -2147023293).
function Format-ErrorCode {
    param($Value)
    $number = [int64]$Value
    if ($number -lt 0) {
        $number += 4294967296
    }
    return ('0x{0:X8}' -f $number)
}

# The COM error code behind an exception (PowerShell wraps it).
function Get-ErrorCode {
    param($Exception)
    $inner = $Exception
    while ($null -ne $inner.InnerException) {
        $inner = $inner.InnerException
    }
    return Format-ErrorCode $inner.HResult
}

# The same update, or a later one of the same kind: the KB number, or the
# title without its numbers.
function Get-UpdateKey {
    param([string]$Title)
    $kb = [regex]::Match($Title, 'KB\d{6,8}')
    if ($kb.Success) {
        return $kb.Value.ToUpperInvariant()
    }
    return ([regex]::Replace($Title, '[\d.]+', '#')).Trim()
}

function Get-KindKey {
    param([string]$Title)
    return ([regex]::Replace($Title, '[\d.]+', '#')).Trim()
}

try {
    $session = New-Object -ComObject Microsoft.Update.Session
    $searcher = $session.CreateUpdateSearcher()
    $total = [int]$searcher.GetTotalHistoryCount()
    $entries = @()
    if ($total -gt 0) {
        $entries = @($searcher.QueryHistory(0, [math]::Min($total, $maxEntries)))
    }
}
catch {
    # 0x80070422: the service is disabled
    if ((Get-ErrorCode $_.Exception) -eq '0x80070422') {
        [pscustomobject]@{ result = 'off'; facts = [ordered]@{} }
        return
    }
    throw
}

# Installations only (Operation 1), newest first. ResultCode: 2 succeeded,
# 3 succeeded with errors, 4 failed, 5 aborted.
$installs = @($entries |
        Where-Object { ([int]$_.Operation -eq 1) -and ($null -ne $_.Date) } |
        Sort-Object -Property { [DateTime]$_.Date } -Descending)
if ($installs.Count -eq 0) {
    [pscustomobject]@{ result = 'empty'; facts = [ordered]@{ last_install = '' } }
    return
}

$since = [DateTime]::UtcNow.AddDays(-$days)
$installedKeys = New-Object System.Collections.Generic.HashSet[string]
$installedKinds = New-Object System.Collections.Generic.HashSet[string]
$failures = New-Object System.Collections.Generic.List[object]
$reported = New-Object System.Collections.Generic.HashSet[string]
$lastInstall = ''
foreach ($entry in $installs) {
    $title = [string]$entry.Title
    $key = Get-UpdateKey $title
    $kind = Get-KindKey $title
    $code = [int]$entry.ResultCode
    if (($code -eq 2) -or ($code -eq 3)) {
        if ($lastInstall.Length -eq 0) {
            $lastInstall = ([DateTime]$entry.Date).ToLocalTime().ToString('yyyy-MM-dd')
        }
        $null = $installedKeys.Add($key)
        $null = $installedKinds.Add($kind)
        continue
    }
    if ((($code -ne 4) -and ($code -ne 5)) -or ([DateTime]$entry.Date -lt $since)) {
        continue
    }
    # Aborted without an error: stopped by a restart or by the user.
    if (($code -eq 5) -and ([int64]$entry.HResult -eq 0)) {
        continue
    }
    # Entries are newest first: a success seen before this one came later.
    if ($installedKeys.Contains($key) -or $installedKinds.Contains($kind) -or (-not $reported.Add($key))) {
        continue
    }
    $failures.Add($entry)
}

if ($failures.Count -eq 0) {
    [pscustomobject]@{ result = 'ok'; facts = [ordered]@{ last_install = $lastInstall } }
    return
}

$latest = $failures[0]
$errorCode = Format-ErrorCode $latest.HResult
$result = $codes[$errorCode]
if ($null -eq $result) {
    $result = 'failed-other'
}
[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        failed_title = [string]$latest.Title
        error_code   = $errorCode
        failed_date  = ([DateTime]$latest.Date).ToLocalTime().ToString('yyyy-MM-dd')
        failed_count = $failures.Count
        last_install = $lastInstall
    }
}
