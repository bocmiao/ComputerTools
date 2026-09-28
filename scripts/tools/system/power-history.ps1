# Tool: system.power-history (info)
# When the PC was started, shut down, restarted, put to sleep and woken up in
# the last $days days, day by day, from the System log. Read-only.
#   Microsoft-Windows-Kernel-General 12   Windows started (boot)
#   Microsoft-Windows-Kernel-Boot 27      how: BootType 0 a full start, 1 fast
#                                         startup (the kernel came back from
#                                         hibernation), 2 resumed from
#                                         hibernation; the 27 nearest to a 12
#                                         (within $bootPairSeconds) belongs to it
#   Microsoft-Windows-Kernel-General 13   Windows shut down; a restart when the
#                                         next start follows within
#                                         $restartSeconds
#   User32 1074                           which program asked for the shutdown
#                                         or restart; belongs to the next 13
#                                         within $initiatorSeconds. Only the
#                                         program's file name is used: the
#                                         event also holds the computer name,
#                                         the user name and the full path.
#   Microsoft-Windows-Kernel-Power 41     Windows had not been shut down
#                                         properly (logged at the next start):
#                                         BugcheckCode non-zero a blue screen,
#                                         PowerButtonTimestamp non-zero or
#                                         LongPowerButtonPressDetected true
#                                         the power button was held, otherwise
#                                         power lost or the PC hung (Microsoft,
#                                         "Advanced troubleshooting for Event
#                                         ID 41")
#   Microsoft-Windows-Kernel-Power 42     going to sleep (or hibernation)
#   Microsoft-Windows-Power-Troubleshooter 1  woken up
# Sections: one per day ("yyyy-MM-dd", newest first, at most $days days with
# records); rows in the order of the day, at most $maxEvents events in all:
# the kind as the row id, the time ("HH:mm") as the value; "by" rows name the
# program behind a shutdown or restart (a code for the well-known ones, the
# file name for others).
# Result codes: found / unexpected (some start followed an improper
# shutdown) / none. Facts: days, boots, unexpected.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 7
$maxEvents = 80
$bootPairSeconds = 120
$restartSeconds = 120
$initiatorSeconds = 120

# Programs that ask for a shutdown or restart, by their file name (lower case).
$initiators = @{
    'explorer.exe'                 = 'by-start'
    'startmenuexperiencehost.exe'  = 'by-start'
    'shellexperiencehost.exe'      = 'by-start'
    'winlogon.exe'                 = 'by-logon'
    'logonui.exe'                  = 'by-logon'
    'shutdown.exe'                 = 'by-command'
    'trustedinstaller.exe'         = 'by-update'
    'mousocoreworker.exe'          = 'by-update'
    'usoclient.exe'                = 'by-update'
    'musnotification.exe'          = 'by-update'
    'musnotificationux.exe'        = 'by-update'
    'wuauclt.exe'                  = 'by-update'
    'tiworker.exe'                 = 'by-update'
    'wininit.exe'                  = 'by-windows'
    'csrss.exe'                    = 'by-windows'
}

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

function Get-SystemEvent {
    param([hashtable]$Filter)
    try {
        return @(Get-WinEvent -FilterHashtable $Filter -ErrorAction Stop)
    }
    catch {
        # Get-WinEvent reports "no events" as an error; that simply means none.
        if ([string]$_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {
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

# A number from an event value ("0x1", "123", "true"), or 0.
function Get-Number {
    param([string]$Text)
    $t = $Text.Trim()
    if ($t -eq 'true') {
        return 1
    }
    $n = [int64]0
    if ($t -match '^0[xX]([0-9A-Fa-f]+)$') {
        return [Convert]::ToInt64($Matches[1], 16)
    }
    if ([int64]::TryParse($t, [ref]$n)) {
        return $n
    }
    return 0
}

# The program behind a 1074: a row (code for the well-known ones, the file name
# for others), or $null.
function Get-InitiatorRow {
    param([hashtable]$Values)
    $process = Get-Text $Values['param1']
    # "C:\Windows\System32\shutdown.exe (COMPUTER)": the path, then the computer.
    $paren = $process.IndexOf(' (')
    if ($paren -gt 0) {
        $process = $process.Substring(0, $paren)
    }
    $name = (($process -split '\\')[-1]).Trim()
    if ($name.Length -eq 0) {
        return $null
    }
    $code = $initiators[$name.ToLowerInvariant()]
    if ($null -ne $code) {
        return [ordered]@{ id = 'by'; code = $code }
    }
    if ($name.Length -gt 60) {
        $name = $name.Substring(0, 60)
    }
    return [ordered]@{ id = 'by'; value = $name }
}

$since = (Get-Date).Date.AddDays(1 - $days)
$wanted = @{
    'Microsoft-Windows-Kernel-General|12'         = 'boot'
    'Microsoft-Windows-Kernel-General|13'         = 'shutdown'
    'Microsoft-Windows-Kernel-Boot|27'            = 'boottype'
    'Microsoft-Windows-Kernel-Power|41'           = 'unexpected'
    'Microsoft-Windows-Kernel-Power|42'           = 'sleep'
    'Microsoft-Windows-Power-Troubleshooter|1'    = 'wake'
    'User32|1074'                                 = 'initiated'
}
$records = New-Object System.Collections.Generic.List[object]
foreach ($record in (Get-SystemEvent @{ LogName = 'System'; Id = 1, 12, 13, 27, 41, 42, 1074; StartTime = $since })) {
    $kind = $wanted[([string]$record.ProviderName + '|' + [string]$record.Id)]
    if ($null -eq $kind) {
        continue
    }
    $records.Add([pscustomobject]@{ Kind = $kind; Time = $record.TimeCreated; Values = (Get-EventValue $record) })
}
# Oldest first.
$ordered = @($records | Sort-Object -Property Time)

$boots = @($ordered | Where-Object { $_.Kind -eq 'boot' })
$bootTypes = @($ordered | Where-Object { $_.Kind -eq 'boottype' })
$initiated = @($ordered | Where-Object { $_.Kind -eq 'initiated' })

# The timeline: Time, Rows.
$timeline = New-Object System.Collections.Generic.List[object]
$unexpectedCount = 0
foreach ($entry in $ordered) {
    $rows = New-Object System.Collections.Generic.List[object]
    $time = $entry.Time.ToString('HH:mm', [Globalization.CultureInfo]::InvariantCulture)
    switch ($entry.Kind) {
        'boot' {
            $id = 'boot'
            $near = @($bootTypes | Where-Object { [Math]::Abs(($_.Time - $entry.Time).TotalSeconds) -le $bootPairSeconds })
            if ($near.Count -gt 0) {
                switch (Get-Number ([string]$near[0].Values['BootType'])) {
                    1 {
                        $id = 'boot-fast'
                    }
                    2 {
                        $id = 'boot-resume'
                    }
                }
            }
            $rows.Add([ordered]@{ id = $id; value = $time })
        }
        'shutdown' {
            $id = 'shutdown'
            $next = @($boots | Where-Object { $_.Time -gt $entry.Time } | Select-Object -First 1)
            if (($next.Count -gt 0) -and (($next[0].Time - $entry.Time).TotalSeconds -le $restartSeconds)) {
                $id = 'restart'
            }
            $rows.Add([ordered]@{ id = $id; value = $time })
            $before = @($initiated | Where-Object { ($_.Time -le $entry.Time) -and (($entry.Time - $_.Time).TotalSeconds -le $initiatorSeconds) })
            if ($before.Count -gt 0) {
                $by = Get-InitiatorRow $before[-1].Values
                if ($null -ne $by) {
                    $rows.Add($by)
                }
            }
        }
        'unexpected' {
            $unexpectedCount++
            $id = 'unexpected'
            $values = $entry.Values
            if ((Get-Number ([string]$values['BugcheckCode'])) -ne 0) {
                $id = 'unexpected-bluescreen'
            }
            elseif (((Get-Number ([string]$values['PowerButtonTimestamp'])) -ne 0) -or ((Get-Number ([string]$values['LongPowerButtonPressDetected'])) -ne 0)) {
                $id = 'unexpected-button'
            }
            $rows.Add([ordered]@{ id = $id; value = $time })
        }
        'sleep' {
            $rows.Add([ordered]@{ id = 'sleep'; value = $time })
        }
        'wake' {
            $rows.Add([ordered]@{ id = 'wake'; value = $time })
        }
    }
    if ($rows.Count -gt 0) {
        $timeline.Add([pscustomobject]@{ Time = $entry.Time; Rows = $rows.ToArray() })
    }
}

# The newest $maxEvents, grouped by day, the newest day first.
$kept = @($timeline | Select-Object -Last $maxEvents)
$sections = New-Object System.Collections.Generic.List[object]
foreach ($group in @($kept | Group-Object -Property { $_.Time.ToString('yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture) } | Sort-Object -Property Name -Descending)) {
    $rows = New-Object System.Collections.Generic.List[object]
    foreach ($item in @($group.Group | Sort-Object -Property Time)) {
        foreach ($row in $item.Rows) {
            $rows.Add($row)
        }
    }
    $sections.Add([ordered]@{ id = 'day'; name = $group.Name; rows = $rows.ToArray() })
}

$result = 'none'
if ($sections.Count -gt 0) {
    $result = 'found'
    if ($unexpectedCount -gt 0) {
        $result = 'unexpected'
    }
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{ days = $days; boots = $boots.Count; unexpected = $unexpectedCount }
    sections = $sections.ToArray()
}
