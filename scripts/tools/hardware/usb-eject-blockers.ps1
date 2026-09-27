# Tool: hardware.usb-eject-blockers (info)
# Who stopped the last "safely remove" of a USB drive: when an eject fails,
# Windows logs event 225 of Microsoft-Windows-Kernel-PnP in the System log
# ("The application ... with process id ... stopped the removal or ejection for
# the device ..."). Read-only; looks at the last $days days, newest first, and
# shows at most $maxShown events.
# The query filters by log, ID and time only and the provider is matched here
# (see checks/disk/error-events.ps1 for why). The event data are not matched by
# field name (not documented): the device is the value that looks like a device
# instance ID, the process ID is the all-digits value, the program is the value
# ending in .exe.
# Privacy: the device instance ID contains the serial number, and the program's
# full path can contain the user name, so neither is output: only the device's
# friendly name (Get-PnpDevice) and the program's file name.
# Rows per event: device, program (file name), what (the file description of
# the running process, or the services in a svchost), running (still running
# now with that process ID), and advice (a code: defender / explorer / search /
# system / service / other).
# Results: none (no such event), found. Facts: count, days.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 7
$maxShown = 5
$since = (Get-Date).AddDays(-$days)

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return (([string]$Value) -replace '[\x00-\x1f]', ' ').Trim()
}

$events = @()
try {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; Id = 225; StartTime = $since } -ErrorAction Stop |
            Where-Object { $_.ProviderName -eq 'Microsoft-Windows-Kernel-PnP' })
}
catch {
    # Get-WinEvent reports "no events" as an error; that simply means none.
    if ([string]$_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {
        $events = @()
    }
    else {
        throw
    }
}

function Get-DeviceName {
    param([string]$InstanceId)
    if ($InstanceId.Length -eq 0) {
        return ''
    }
    try {
        $device = Get-PnpDevice -InstanceId $InstanceId -ErrorAction Stop
        return Get-Text $device.FriendlyName
    }
    catch {
        return ''
    }
}

function Get-Advice {
    param([string]$Program)
    switch ($Program.ToLowerInvariant()) {
        'msmpeng.exe' { return 'defender' }
        'mpdefendercoreservice.exe' { return 'defender' }
        'explorer.exe' { return 'explorer' }
        'searchindexer.exe' { return 'search' }
        'searchprotocolhost.exe' { return 'search' }
        'searchfilterhost.exe' { return 'search' }
        'system' { return 'system' }
        'svchost.exe' { return 'service' }
        default { return 'other' }
    }
}

$sections = New-Object System.Collections.Generic.List[object]
foreach ($entry in $events) {
    if ($sections.Count -ge $maxShown) {
        break
    }
    $values = @()
    try {
        $xml = [xml]$entry.ToXml()
        $values = @($xml.Event.EventData.Data | ForEach-Object { Get-Text $_.'#text' })
    }
    catch {
        $values = @($entry.Properties | ForEach-Object { Get-Text $_.Value })
    }
    $device = ''
    $processId = 0
    $program = ''
    foreach ($v in $values) {
        if (($device.Length -eq 0) -and ($v -match '^(USB|USBSTOR|SCSI|STORAGE|SWD|SD|UASPSTOR|WPDBUSENUM)\\')) {
            $device = $v
        }
        elseif (($processId -eq 0) -and ($v -match '^\d+$')) {
            $processId = [int]$v
        }
        elseif (($program.Length -eq 0) -and ($v -match '(?i)\.exe$')) {
            $program = $v.Substring($v.LastIndexOf('\') + 1)
        }
    }
    if (($program.Length -eq 0) -and ($processId -eq 4)) {
        $program = 'System'
    }
    $rows = New-Object System.Collections.Generic.List[object]
    $name = Get-DeviceName $device
    if ($name.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'device'; value = $name })
    }
    if ($program.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'program'; value = $program })
    }
    # Still running, as the same program? Then say what it is.
    $running = $false
    if ($processId -gt 0) {
        $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
        $same = ($null -ne $process) -and (($program.Length -eq 0) -or ($program -ieq ($process.ProcessName + '.exe')) -or ($program -ieq $process.ProcessName))
        if ($same) {
            $running = $true
            $what = ''
            if ($program -ieq 'svchost.exe') {
                $services = @(Get-CimInstance -ClassName Win32_Service -Filter ('ProcessId = {0}' -f $processId) -ErrorAction SilentlyContinue |
                        ForEach-Object { Get-Text $_.DisplayName } | Where-Object { $_.Length -gt 0 } | Select-Object -First 4)
                $what = $services -join ', '
            }
            else {
                try {
                    $what = Get-Text $process.MainModule.FileVersionInfo.FileDescription
                }
                catch {
                    $what = ''
                }
            }
            if ($what.Length -gt 0) {
                $rows.Add([ordered]@{ id = 'what'; value = $what })
            }
        }
    }
    $runningCode = 'no'
    if ($running) {
        $runningCode = 'yes'
    }
    $rows.Add([ordered]@{ id = 'running'; code = $runningCode })
    $rows.Add([ordered]@{ id = 'advice'; code = (Get-Advice $program) })
    $sections.Add([ordered]@{
            id   = 'event'
            name = $entry.TimeCreated.ToString('yyyy-MM-dd HH:mm')
            rows = $rows.ToArray()
        })
}

$result = 'none'
if ($events.Count -gt 0) {
    $result = 'found'
}
[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{ count = $events.Count; days = $days }
    sections = $sections.ToArray()
}
