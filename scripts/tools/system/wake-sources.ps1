# Tool: system.wake-sources (info)
# What woke this PC? Read-only.
# - The last wakes from sleep (up to 5 in 14 days): event 1 of
#   Microsoft-Windows-Power-Troubleshooter in the System log. The query
#   filters by log, ID and time and the provider is matched here (asking
#   Get-WinEvent for a provider fails on some systems; see
#   checks/disk/error-events.ps1). From its EventData: WakeSourceText (a
#   device's name, or empty) and WakeTimerContext / WakeTimerOwner (what set
#   the wake timer). WakeSourceType is not used: its numbers differ between
#   Windows builds.
# - The devices allowed to wake the PC: MSPower_DeviceWakeEnable in root\wmi
#   with Enable = TRUE, the "Allow this device to wake the computer" box of
#   Device Manager (up to 10), named from Win32_PnPEntity.
# - Scheduled tasks that may wake the PC to run (Settings.WakeToRun) and are
#   not disabled (up to 10).
# - "Allow wake timers" of the active plan (see the shared block): 0 No,
#   1 Yes, 2 Important.
# Privacy: of a program's path only the file name is kept, and a user folder
# name or a user's SID in any text is replaced.
# Result codes: found (a wake in the last 14 days) / none.
# Facts: days, wakes, devices, tasks.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block power-plan: identical in checks/system/power-plan.ps1, checks/system/wake-timers.ps1, checks/system/power-timeouts.ps1, checks/system/lid-action.ps1, checks/hardware/usb-suspend.ps1, tools/system/wake-sources.ps1 and features/power/*.ps1 (medkit-data check compares them) ----
# The active power plan: ActivePowerScheme under
# HKLM\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes (a GUID). The
# built-in plans: Power saver a1841308-3541-4fab-bc81-f71556f20b4a, Balanced
# 381b4222-f694-41f0-9685-ff5bb260df2e, High performance
# 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c, Ultimate performance
# e9a42b02-d5df-448d-aa00-03f14749eb61.
# A power setting of a plan, plugged in (AC) or on battery (DC): the value
# Windows uses, from root\cimv2\power Win32_PowerSettingDataIndex (the setting
# GUID alone names it there; its subgroup does not).
# powercfg.exe of this Windows (the 64-bit one, also from a 32-bit
# PowerShell) changes them. Its output is localized and is not read: success
# is judged by the exit code, and the callers read the values back.
$powerSchemesKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes'

function Get-ActivePlan {
    $value = (Get-ItemProperty -LiteralPath $powerSchemesKey -Name 'ActivePowerScheme' -ErrorAction Stop).ActivePowerScheme
    return ([string]$value).Trim().Trim('{', '}').ToLowerInvariant()
}

# $Power is AC or DC; $null when Windows does not have the setting
function Get-PowerSetting {
    param([string]$Plan, [string]$Power, [string]$Setting)
    $id = 'Microsoft:PowerSettingDataIndex\{' + $Plan + '}\' + $Power + '\{' + $Setting + '}'
    $filter = "InstanceID = '" + $id.Replace('\', '\\') + "'"
    $item = @(Get-CimInstance -Namespace 'root\cimv2\power' -ClassName 'Win32_PowerSettingDataIndex' -Filter $filter -ErrorAction SilentlyContinue)
    if ($item.Count -eq 0) {
        return $null
    }
    return [int]$item[0].SettingIndexValue
}

$powerSystemDir = $env:SystemRoot + '\System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $powerSystemDir = $env:SystemRoot + '\Sysnative'
}

function Invoke-Powercfg {
    param([string[]]$Arguments)
    # With ErrorActionPreference 'Stop', Windows PowerShell 5.1 would turn any
    # stderr line of a native command into a terminating error.
    $ErrorActionPreference = 'Continue'
    $null = & ($powerSystemDir + '\powercfg.exe') @Arguments 2>&1
    return $LASTEXITCODE
}

# Sets a power setting of $Plan (in subgroup $Group) plugged in and on battery,
# and makes the plan active again so that Windows uses it now; $true when
# every call worked
function Set-PowerSetting {
    param([string]$Plan, [string]$Group, [string]$Setting, [int]$Ac, [int]$Dc)
    $ok = (Invoke-Powercfg @('/setacvalueindex', $Plan, $Group, $Setting, [string]$Ac)) -eq 0
    $ok = ((Invoke-Powercfg @('/setdcvalueindex', $Plan, $Group, $Setting, [string]$Dc)) -eq 0) -and $ok
    $ok = ((Invoke-Powercfg @('/setactive', $Plan)) -eq 0) -and $ok
    return $ok
}
# ---- end of shared block power-plan ----

$wakeTimers = 'bd3b718a-0680-4d9d-8ab2-e1d2b4ac806d'

$days = 14
$maxItems = 10
$timerCodes = @{ 0 = 'disabled'; 1 = 'enabled'; 2 = 'important' }

function Get-CleanText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    $text = ((([string]$Value) -replace '[\x00-\x1f]', ' ') -replace '\s+', ' ').Trim()
    # A user folder name is personal, and so is the user's SID (some task
    # names end with it)
    $text = $text -replace '(?i)(\\(Users|Documents and Settings)\\)[^\\]+', '$1*'
    $text = $text -replace '(?i)S-1-5-21(-\d+)+', '*'
    if ($text.Length -gt 160) {
        $text = $text.Substring(0, 160)
    }
    return $text
}

$sections = New-Object System.Collections.Generic.List[object]

# The last wakes
$events = @()
try {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = 'System'; Id = 1; StartTime = (Get-Date).AddDays(-$days) } -ErrorAction Stop |
            Where-Object { $_.ProviderName -eq 'Microsoft-Windows-Power-Troubleshooter' } |
            Select-Object -First 5)
}
catch {
    if ([string]$_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') {
        throw
    }
}
foreach ($record in $events) {
    $values = @{}
    try {
        $xml = [xml]$record.ToXml()
        foreach ($data in @($xml.Event.EventData.Data)) {
            if (($null -ne $data) -and ($data -isnot [string])) {
                $values[[string]$data.GetAttribute('Name')] = [string]$data.InnerText
            }
        }
    }
    catch {
        Write-Verbose ('An event could not be read: {0}' -f $_.Exception.Message)
    }
    $rows = New-Object System.Collections.Generic.List[object]
    $source = Get-CleanText $values['WakeSourceText']
    if ($source.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'source'; value = $source })
    }
    $timer = Get-CleanText $values['WakeTimerContext']
    if ($timer.Length -eq 0) {
        # A program's path: only its file name
        $timer = Get-CleanText ((([string]$values['WakeTimerOwner']) -split '\\')[-1])
    }
    if ($timer.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'timer'; value = $timer })
    }
    if ($rows.Count -eq 0) {
        $rows.Add([ordered]@{ id = 'source'; code = 'unknown' })
    }
    $when = $record.TimeCreated.ToString('yyyy-MM-dd HH:mm', [Globalization.CultureInfo]::InvariantCulture)
    $sections.Add([ordered]@{ id = 'wake'; name = $when; rows = $rows.ToArray() })
}

# Devices allowed to wake the PC
$deviceNames = New-Object System.Collections.Generic.List[string]
try {
    $armed = @(Get-CimInstance -Namespace 'root\wmi' -ClassName 'MSPower_DeviceWakeEnable' -ErrorAction Stop | Where-Object { $_.Enable -eq $true })
    if ($armed.Count -gt 0) {
        $names = @{}
        foreach ($device in @(Get-CimInstance -ClassName 'Win32_PnPEntity' -ErrorAction Stop)) {
            $id = [string]$device.PNPDeviceID
            if ($id.Length -gt 0) {
                $names[$id.ToUpperInvariant()] = Get-CleanText $device.Name
            }
        }
        foreach ($item in $armed) {
            # InstanceName is the device instance ID with "_0" appended
            $id = ([string]$item.InstanceName -replace '_\d+$', '').ToUpperInvariant()
            $name = $names[$id]
            if ((-not [string]::IsNullOrEmpty($name)) -and (-not $deviceNames.Contains($name)) -and ($deviceNames.Count -lt $maxItems)) {
                $deviceNames.Add($name)
            }
        }
    }
}
catch {
    Write-Verbose ('The devices could not be listed: {0}' -f $_.Exception.Message)
}
$rows = New-Object System.Collections.Generic.List[object]
foreach ($name in $deviceNames) {
    $rows.Add([ordered]@{ id = 'device'; value = $name })
}
if ($rows.Count -eq 0) {
    $rows.Add([ordered]@{ id = 'device'; code = 'none' })
}
$sections.Add([ordered]@{ id = 'devices'; rows = $rows.ToArray() })

# Scheduled tasks that may wake the PC
$taskNames = New-Object System.Collections.Generic.List[string]
try {
    foreach ($task in @(Get-ScheduledTask -ErrorAction Stop)) {
        if (($task.Settings.WakeToRun -eq $true) -and ([string]$task.State -ne 'Disabled') -and ($taskNames.Count -lt $maxItems)) {
            $taskNames.Add((Get-CleanText ([string]$task.TaskPath + [string]$task.TaskName)))
        }
    }
}
catch {
    Write-Verbose ('The scheduled tasks could not be listed: {0}' -f $_.Exception.Message)
}
$rows = New-Object System.Collections.Generic.List[object]
foreach ($name in $taskNames) {
    $rows.Add([ordered]@{ id = 'task'; value = $name })
}
if ($rows.Count -eq 0) {
    $rows.Add([ordered]@{ id = 'task'; code = 'none' })
}
$sections.Add([ordered]@{ id = 'tasks'; rows = $rows.ToArray() })

# Allow wake timers
$rows = New-Object System.Collections.Generic.List[object]
try {
    $plan = Get-ActivePlan
    foreach ($power in @('AC', 'DC')) {
        $value = Get-PowerSetting $plan $power $wakeTimers
        $code = 'unknown'
        if (($null -ne $value) -and $timerCodes.ContainsKey($value)) {
            $code = $timerCodes[$value]
        }
        $rows.Add([ordered]@{ id = $power.ToLowerInvariant(); code = $code })
    }
}
catch {
    Write-Verbose ('The power plan could not be read: {0}' -f $_.Exception.Message)
}
if ($rows.Count -gt 0) {
    $sections.Add([ordered]@{ id = 'timers'; rows = $rows.ToArray() })
}

$result = 'none'
if ($events.Count -gt 0) {
    $result = 'found'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{ days = $days; wakes = $events.Count; devices = $deviceNames.Count; tasks = $taskNames.Count }
    sections = $sections.ToArray()
}
