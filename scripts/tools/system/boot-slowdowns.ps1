# Tool: system.boot-slowdowns (info)
# What made starting (and shutting down) the PC slow in the last $days days,
# from the Microsoft-Windows-Diagnostics-Performance/Operational log. Windows
# times every start-up and shutdown; next to the timing it logs one event for
# each program, driver, service or device that took clearly longer than it
# usually does (Microsoft, "Event ID 101: Windows Diagnostics Performance"):
#   100  start-up timing: MainPathBootTime (power on until the desktop, ms),
#        BootTime (that plus BootPostBootTime, until the PC is idle),
#        BootNumStartupApps, BootIsRebootAfterInstall
#   101  a program started slower than usual     102  a driver
#   103  a start-up service                      109  a device
#   201  a program made shutting down slow       202  a device
#   203  a service
# 101-203 carry Name (file, service or device name), FriendlyName (the
# description), TotalTime and DegradationTime (ms: how long it took, how much
# longer than usual it took) and, for files, Path, ProductName and CompanyName.
# 104-108 and 110 (Windows' own start-up phases, prefetching, group policy) are
# left out: there is nothing to do about them at home.
# Grouped by phase, kind and name. The ones that are not Windows' own come
# first (there is something to do about them), then the most time lost (the sum
# of DegradationTime); at most $maxBoot for start-up and $maxShutdown for
# shutdown.
# Windows' own: CompanyName starts with Microsoft; or, for programs and
# services without a company, the file is in the Windows folder (drivers of
# other makers live there too, devices never count).
# Privacy: Path is only used for that and never output (it can hold the user
# name); names that hold a path are cut to its last part; the Name of a
# device (its instance ID, which can hold a serial number) is never used.
# The summary is the newest start-up (event 100, also older than $days days).
# Its row boot_count is labelled "in the last 30 days": keep that in step
# with $days.
# Results: found (at least one item that is not Windows' own) / windows-only /
# none (nothing was slower than usual) / no-data (no event 100 at all: the log
# does not exist, as on Windows Server, or has been disabled or emptied).
# Facts: days, boots (start-ups in the last $days days), items, others (not
# Windows' own), shutdown_items; desktop_sec and startup_apps of the newest
# start-up when it has them.
# Read-only. Needs administrator rights (the log is restricted).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 30
$maxBoot = 8
$maxShutdown = 5
$logName = 'Microsoft-Windows-Diagnostics-Performance/Operational'
$since = (Get-Date).AddDays(-$days)
$invariant = [Globalization.CultureInfo]::InvariantCulture

# Event ID -> phase, kind.
$kinds = @{
    101 = @('boot', 'app')
    102 = @('boot', 'driver')
    103 = @('boot', 'service')
    109 = @('boot', 'device')
    201 = @('shutdown', 'app')
    202 = @('shutdown', 'device')
    203 = @('shutdown', 'service')
}

$windowsDir = ([string]$env:SystemRoot).TrimEnd('\')

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return (([string]$Value) -replace '[\x00-\x1f]', ' ').Trim()
}

# The last part of a path ("C:\Users\x\a.exe" -> "a.exe").
function Get-Leaf {
    param([string]$Text)
    $i = $Text.LastIndexOfAny([char[]]@('\', '/'))
    if ($i -ge 0) {
        return $Text.Substring($i + 1).Trim()
    }
    return $Text
}

function Get-LogEvent {
    param([hashtable]$Filter, [int]$Max = 0)
    try {
        if ($Max -gt 0) {
            return @(Get-WinEvent -FilterHashtable $Filter -MaxEvents $Max -ErrorAction Stop)
        }
        return @(Get-WinEvent -FilterHashtable $Filter -ErrorAction Stop)
    }
    catch {
        # Get-WinEvent reports "no such log" and "no events" as errors; both
        # simply mean none.
        $errorId = [string]$_.FullyQualifiedErrorId
        if (($errorId -like 'NoMatchingEventsFound*') -or ($errorId -like 'NoMatchingLogsFound*')) {
            return @()
        }
        throw
    }
}

# The named values of an event (EventData), as text.
function Get-EventValue {
    param($Record)
    $values = @{}
    try {
        $xml = [xml]$Record.ToXml()
        foreach ($data in @($xml.Event.EventData.Data)) {
            if (($null -eq $data) -or ($data -is [string])) {
                continue
            }
            $name = Get-Text $data.GetAttribute('Name')
            if ($name.Length -gt 0) {
                $values[$name] = Get-Text $data.InnerText
            }
        }
    }
    catch {
        return @{}
    }
    return $values
}

# Milliseconds written as a number, or 0.
function ConvertTo-Ms {
    param([string]$Text)
    $number = [double]0
    if ([double]::TryParse($Text, [Globalization.NumberStyles]::Float, $invariant, [ref]$number) -and ($number -ge 0)) {
        return $number
    }
    return [double]0
}

function ConvertTo-Seconds {
    param([double]$Ms)
    return [math]::Round($Ms / 1000, 1)
}

function Format-Time {
    param([datetime]$Time)
    return $Time.ToString('yyyy-MM-dd HH:mm', $invariant)
}

function Test-WindowsOwn {
    param([string]$Kind, [string]$Company, [string]$Path)
    if ($Kind -eq 'device') {
        return $false
    }
    if ($Company -match '^Microsoft') {
        return $true
    }
    if (($Company.Length -gt 0) -or ($Kind -eq 'driver') -or ($windowsDir.Length -eq 0) -or ($Path.Length -eq 0)) {
        return $false
    }
    $full = $Path
    if ($full -match '^\\SystemRoot\\(.*)$') {
        $full = $windowsDir + '\' + $Matches[1]
    }
    elseif ($full -match '^\\\?\?\\(.*)$') {
        $full = $Matches[1]
    }
    $full = [Environment]::ExpandEnvironmentVariables($full)
    return $full.StartsWith($windowsDir + '\', [StringComparison]::OrdinalIgnoreCase)
}

$latest = @(Get-LogEvent @{ LogName = $logName; Id = 100 } 1)
if ($latest.Count -eq 0) {
    [pscustomobject]@{ result = 'no-data'; facts = [ordered]@{ days = $days } }
    return
}
$ids = @(100) + @($kinds.Keys | Sort-Object)
$events = @(Get-LogEvent @{ LogName = $logName; Id = $ids; StartTime = $since })

$boots = 0
$groups = @{}
# Get-WinEvent returns the newest events first: names and makers come from the
# newest event of each item.
foreach ($record in $events) {
    $id = [int]$record.Id
    if ($id -eq 100) {
        $boots++
        continue
    }
    if (-not $kinds.ContainsKey($id)) {
        continue
    }
    $time = $record.TimeCreated
    if ($null -eq $time) {
        continue
    }
    $phase = $kinds[$id][0]
    $kind = $kinds[$id][1]
    $values = Get-EventValue $record
    $friendly = Get-Text $values['FriendlyName']
    if ($friendly -match '(^\\|[A-Za-z]:\\)') {
        $friendly = Get-Leaf $friendly
    }
    $name = ''
    if ($kind -ne 'device') {
        $name = Get-Leaf (Get-Text $values['Name'])
    }
    $keyName = $name
    if ($keyName.Length -eq 0) {
        $keyName = $friendly
    }
    if ($keyName.Length -eq 0) {
        continue
    }
    $key = $phase + '|' + $kind + '|' + $keyName.ToLowerInvariant()
    if (-not $groups.ContainsKey($key)) {
        $company = Get-Text $values['CompanyName']
        $groups[$key] = [pscustomobject]@{
            Phase      = $phase
            Kind       = $kind
            Name       = $name
            Friendly   = $friendly
            Company    = $company
            Windows    = (Test-WindowsOwn $kind $company (Get-Text $values['Path']))
            Times      = 0
            Sum        = [double]0
            Worst      = [double]-1
            WorstTotal = [double]0
            Last       = $time
        }
    }
    $group = $groups[$key]
    $slower = ConvertTo-Ms $values['DegradationTime']
    $group.Times++
    $group.Sum += $slower
    if ($slower -gt $group.Worst) {
        $group.Worst = $slower
        $group.WorstTotal = ConvertTo-Ms $values['TotalTime']
    }
    if ($time -gt $group.Last) {
        $group.Last = $time
    }
    if (($group.Friendly.Length -eq 0) -and ($friendly.Length -gt 0)) {
        $group.Friendly = $friendly
    }
}

$sections = New-Object System.Collections.Generic.List[object]

# ---- the newest start-up
$bootValues = Get-EventValue $latest[0]
$desktopMs = ConvertTo-Ms $bootValues['MainPathBootTime']
if ($desktopMs -le 0) {
    $desktopMs = ConvertTo-Ms $bootValues['BootTime']
}
$postMs = ConvertTo-Ms $bootValues['BootPostBootTime']
$startupApps = -1
$count = 0
if ([int]::TryParse([string]$bootValues['BootNumStartupApps'], [Globalization.NumberStyles]::None, $invariant, [ref]$count)) {
    $startupApps = $count
}
$rows = New-Object System.Collections.Generic.List[object]
if ($null -ne $latest[0].TimeCreated) {
    $rows.Add([ordered]@{ id = 'when'; value = (Format-Time $latest[0].TimeCreated) })
}
if ([string]$bootValues['BootIsRebootAfterInstall'] -eq 'true') {
    $rows.Add([ordered]@{ id = 'after_update'; value = $true })
}
if ($desktopMs -gt 0) {
    $rows.Add([ordered]@{ id = 'desktop'; value = (ConvertTo-Seconds $desktopMs) })
}
if ($postMs -gt 0) {
    $rows.Add([ordered]@{ id = 'busy'; value = (ConvertTo-Seconds $postMs) })
}
if ($startupApps -ge 0) {
    $rows.Add([ordered]@{ id = 'startup_apps'; value = $startupApps })
}
$rows.Add([ordered]@{ id = 'boot_count'; value = $boots })
$sections.Add([ordered]@{ id = 'boots'; rows = $rows.ToArray() })

# ---- what was slower than usual
$sorted = @($groups.Values | Sort-Object -Property @(
        @{ Expression = 'Windows'; Descending = $false },
        @{ Expression = 'Sum'; Descending = $true },
        @{ Expression = 'Last'; Descending = $true }
    ))
$shown = @($sorted | Where-Object { $_.Phase -eq 'boot' } | Select-Object -First $maxBoot) +
    @($sorted | Where-Object { $_.Phase -eq 'shutdown' } | Select-Object -First $maxShutdown)
foreach ($group in $shown) {
    $kindCode = $group.Kind
    $advice = $group.Kind + '-advice'
    if ($group.Windows) {
        $kindCode = 'windows-' + $group.Kind
        $advice = 'windows-advice'
    }
    if ($group.Phase -eq 'shutdown') {
        $advice = 'shutdown-' + $advice
    }
    $rows = New-Object System.Collections.Generic.List[object]
    $rows.Add([ordered]@{ id = 'kind'; code = $kindCode })
    $display = $group.Friendly
    if ($display.Length -eq 0) {
        $display = $group.Name
    }
    elseif (($group.Name.Length -gt 0) -and ($group.Name -ne $group.Friendly)) {
        $rows.Add([ordered]@{ id = 'file'; value = $group.Name })
    }
    if ($group.Company.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'maker'; value = $group.Company })
    }
    $rows.Add([ordered]@{ id = 'times'; value = $group.Times })
    $rows.Add([ordered]@{ id = 'slower'; value = (ConvertTo-Seconds ([math]::Max($group.Worst, 0))) })
    if ($group.WorstTotal -gt 0) {
        $rows.Add([ordered]@{ id = 'took'; value = (ConvertTo-Seconds $group.WorstTotal) })
    }
    $rows.Add([ordered]@{ id = 'last'; value = (Format-Time $group.Last) })
    $rows.Add([ordered]@{ id = 'advice'; code = $advice })
    $section = [ordered]@{ id = $group.Phase }
    if ($display.Length -gt 0) {
        $section['name'] = $display
    }
    $section['rows'] = $rows.ToArray()
    $sections.Add($section)
}

$others = @($groups.Values | Where-Object { -not $_.Windows }).Count
$result = 'none'
if ($others -gt 0) {
    $result = 'found'
}
elseif ($groups.Count -gt 0) {
    $result = 'windows-only'
}

$facts = [ordered]@{
    days           = $days
    boots          = $boots
    items          = $groups.Count
    others         = $others
    shutdown_items = @($groups.Values | Where-Object { $_.Phase -eq 'shutdown' }).Count
}
if ($desktopMs -gt 0) {
    $facts['desktop_sec'] = ConvertTo-Seconds $desktopMs
}
if ($startupApps -ge 0) {
    $facts['startup_apps'] = $startupApps
}

[pscustomobject]@{
    result   = $result
    facts    = $facts
    sections = $sections.ToArray()
}
