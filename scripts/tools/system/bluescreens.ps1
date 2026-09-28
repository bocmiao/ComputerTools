# Tool: system.bluescreens (info)
# Blue screens and unexpected restarts of the last $days days, from the
# System log. Read-only.
#   Blue screens (bug checks), from two events logged when the PC starts again:
#     Microsoft-Windows-WER-SystemErrorReporting 1001 (BugCheck 1001 on older
#       Windows), logged when the crash dump was saved; its first value is
#       "0x0000009f (0x..., 0x..., 0x..., 0x...)": the stop code, then its
#       parameters.
#     Microsoft-Windows-Kernel-Power 41 ("rebooted without cleanly shutting
#       down first"): BugcheckCode (decimal) is the stop code when Windows
#       could record it, also when no dump was saved (Microsoft, "Event ID
#       41"). A 41 and a 1001 with the same code, the 1001 logged up to
#       $pairMinutes minutes after the 41, are the same blue screen.
#   Grouped by stop code, the newest first, at most $maxCodes codes: the name
#   (Microsoft's bug check code reference, $stops), how many times, the first
#   and the last time, the likely cause and what to do (a code per group of
#   stop codes; codes not in $stops are shown as they are).
#   For a few codes the four parameters tell more (the newest blue screen of
#   the group whose parameters say something is used; see Get-Detail):
#     0x7A (and 0x77 with a status as parameter 1): parameter 2 is the I/O
#       status: C000009C / C000016A bad sectors, C000009D / C0000185 /
#       C000000E the disk is not connected properly (cable, controller) or
#       failed, C000009A out of nonpaged pool
#     0xA0 with parameter 1 = 0xB: the hibernation file is too small
#     0xFE with parameter 1 = 3 or 5: the USB controller reported a hardware
#       failure; with 8, 6 and a timeout code about (selective) suspend or
#       resume (1-5, 8, 9): a USB port or hub did not wake up in time
#   The parameters come from the 1001 message ("(0x..., 0x..., 0x..., 0x...)")
#   or from BugcheckParameter1-4 of the 41.
#   Unexpected shutdowns: 41 with BugcheckCode 0 (power lost, hard hang;
#   PowerButtonTimestamp non-zero or LongPowerButtonPressDetected: the power
#   button was held).
#   Restarts by Windows Update: User32 1074 ("The process X has initiated the
#   restart / power off ...") whose process is one of $updatePrograms. Only
#   counted: 1074 also holds the computer name, the user name and the path.
#   Crash dumps off: volmgr 46 ("Crash dump initialization failed!") since the
#   last start: no page file on the system drive (or dump file) to save a crash
#   dump to, so a blue screen leaves no record (Microsoft, "Event ID 46"). The
#   page file was turned off or moved to another drive; GitHub's Windows VMs
#   keep it on the temporary drive D:, so CI shows this.
# Times are those of the events, that is when the PC started again.
# The query filters by log, ID and time only and the provider is matched here
# (see checks/disk/error-events.ps1 for why).
# Results: listed (blue screens) / shutdowns (unexpected shutdowns, no blue
# screen) / no-dump (nothing recorded, but a blue screen would not be) /
# updates (only restarts by Windows Update) / none.
# Facts: days, count (blue screens), codes (distinct stop codes), shutdowns,
# power_button, updates, dump_off.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 30
$maxCodes = 10
$pairMinutes = 30
$since = (Get-Date).AddDays(-$days)
$invariant = [Globalization.CultureInfo]::InvariantCulture

# Stop code (8 hex digits, upper case) -> name, as in Microsoft's bug check
# code reference, and the group it belongs to (the cause and the advice).
$stops = @{
    '0000000A' = @('IRQL_NOT_LESS_OR_EQUAL', 'driver')
    '00000019' = @('BAD_POOL_HEADER', 'driver')
    '0000001A' = @('MEMORY_MANAGEMENT', 'memory')
    '0000001E' = @('KMODE_EXCEPTION_NOT_HANDLED', 'driver')
    '00000023' = @('FAT_FILE_SYSTEM', 'disk')
    '00000024' = @('NTFS_FILE_SYSTEM', 'disk')
    '0000002E' = @('DATA_BUS_ERROR', 'memory')
    '0000003B' = @('SYSTEM_SERVICE_EXCEPTION', 'driver')
    '0000003D' = @('INTERRUPT_EXCEPTION_NOT_HANDLED', 'driver')
    '0000004E' = @('PFN_LIST_CORRUPT', 'memory')
    '00000050' = @('PAGE_FAULT_IN_NONPAGED_AREA', 'memory')
    '00000051' = @('REGISTRY_ERROR', 'system')
    '00000053' = @('NO_BOOT_DEVICE', 'boot')
    '0000005A' = @('CRITICAL_SERVICE_FAILED', 'system')
    '00000074' = @('BAD_SYSTEM_CONFIG_INFO', 'system')
    '00000077' = @('KERNEL_STACK_INPAGE_ERROR', 'disk')
    '0000007A' = @('KERNEL_DATA_INPAGE_ERROR', 'disk')
    '0000007B' = @('INACCESSIBLE_BOOT_DEVICE', 'boot')
    '0000007C' = @('BUGCODE_NDIS_DRIVER', 'network')
    '0000007E' = @('SYSTEM_THREAD_EXCEPTION_NOT_HANDLED', 'driver')
    '0000007F' = @('UNEXPECTED_KERNEL_MODE_TRAP', 'hardware')
    '00000080' = @('NMI_HARDWARE_FAILURE', 'hardware')
    '0000008E' = @('KERNEL_MODE_EXCEPTION_NOT_HANDLED', 'driver')
    '0000009C' = @('MACHINE_CHECK_EXCEPTION', 'hardware')
    '0000009F' = @('DRIVER_POWER_STATE_FAILURE', 'power')
    '000000A0' = @('INTERNAL_POWER_ERROR', 'power')
    '000000A5' = @('ACPI_BIOS_ERROR', 'bios')
    '000000B4' = @('VIDEO_DRIVER_INIT_FAILURE', 'gpu')
    '000000BE' = @('ATTEMPTED_WRITE_TO_READONLY_MEMORY', 'driver')
    '000000C1' = @('SPECIAL_POOL_DETECTED_MEMORY_CORRUPTION', 'verifier')
    '000000C2' = @('BAD_POOL_CALLER', 'driver')
    '000000C4' = @('DRIVER_VERIFIER_DETECTED_VIOLATION', 'verifier')
    '000000C5' = @('DRIVER_CORRUPTED_EXPOOL', 'driver')
    '000000C9' = @('DRIVER_VERIFIER_IOMANAGER_VIOLATION', 'verifier')
    '000000CA' = @('PNP_DETECTED_FATAL_ERROR', 'driver')
    '000000D1' = @('DRIVER_IRQL_NOT_LESS_OR_EQUAL', 'driver')
    '000000D5' = @('DRIVER_PAGE_FAULT_IN_FREED_SPECIAL_POOL', 'verifier')
    '000000E0' = @('ACPI_BIOS_FATAL_ERROR', 'bios')
    '000000E2' = @('MANUALLY_INITIATED_CRASH', 'manual')
    '000000EA' = @('THREAD_STUCK_IN_DEVICE_DRIVER', 'gpu')
    '000000ED' = @('UNMOUNTABLE_BOOT_VOLUME', 'boot')
    '000000EF' = @('CRITICAL_PROCESS_DIED', 'system')
    '000000F0' = @('STORAGE_MINIPORT_ERROR', 'disk')
    '000000F2' = @('HARDWARE_INTERRUPT_STORM', 'hardware')
    '000000F4' = @('CRITICAL_OBJECT_TERMINATION', 'system')
    '000000F7' = @('DRIVER_OVERRAN_STACK_BUFFER', 'driver')
    '000000FC' = @('ATTEMPTED_EXECUTE_OF_NOEXECUTE_MEMORY', 'driver')
    '000000FE' = @('BUGCODE_USB_DRIVER', 'usb')
    '00000101' = @('CLOCK_WATCHDOG_TIMEOUT', 'hardware')
    '00000102' = @('DPC_WATCHDOG_TIMEOUT', 'driver')
    '00000108' = @('THIRD_PARTY_FILE_SYSTEM_FAILURE', 'driver')
    '00000109' = @('CRITICAL_STRUCTURE_CORRUPTION', 'driver')
    '0000010D' = @('WDF_VIOLATION', 'driver')
    '0000010E' = @('VIDEO_MEMORY_MANAGEMENT_INTERNAL', 'gpu')
    '00000113' = @('VIDEO_DXGKRNL_FATAL_ERROR', 'gpu')
    '00000114' = @('VIDEO_SHADOW_DRIVER_FATAL_ERROR', 'gpu')
    '00000116' = @('VIDEO_TDR_FAILURE', 'gpu')
    '00000117' = @('VIDEO_TDR_TIMEOUT_DETECTED', 'gpu')
    '00000119' = @('VIDEO_SCHEDULER_INTERNAL_ERROR', 'gpu')
    '00000124' = @('WHEA_UNCORRECTABLE_ERROR', 'hardware')
    '0000012B' = @('FAULTY_HARDWARE_CORRUPTED_PAGE', 'memory')
    '0000012C' = @('EXFAT_FILE_SYSTEM', 'disk')
    '00000133' = @('DPC_WATCHDOG_VIOLATION', 'driver')
    '00000139' = @('KERNEL_SECURITY_CHECK_FAILURE', 'driver')
    '0000013A' = @('KERNEL_MODE_HEAP_CORRUPTION', 'driver')
    '00000140' = @('STORAGE_DEVICE_ABNORMALITY_DETECTED', 'disk')
    '00000144' = @('BUGCODE_USB3_DRIVER', 'usb')
    '0000014C' = @('FATAL_ABNORMAL_RESET_ERROR', 'hardware')
    '00000154' = @('UNEXPECTED_STORE_EXCEPTION', 'disk')
    '00000164' = @('WIN32K_CRITICAL_FAILURE', 'driver')
    '0000017E' = @('MICROCODE_REVISION_MISMATCH', 'bios')
    '00000187' = @('VIDEO_DWMINIT_TIMEOUT_FALLBACK_BDD', 'gpu')
    '000001C7' = @('STORE_DATA_STRUCTURE_CORRUPTION', 'memory')
    '000001C8' = @('MANUALLY_INITIATED_POWER_BUTTON_HOLD', 'hang')
    '000001CF' = @('HARDWARE_WATCHDOG_TIMEOUT', 'hardware')
    '000001D5' = @('DRIVER_PNP_WATCHDOG', 'driver')
    '000001DE' = @('BUGCODE_WIFIADAPTER_DRIVER', 'network')
    '000001E4' = @('VIDEO_DXGKRNL_SYSMM_FATAL_ERROR', 'gpu')
    '1000007E' = @('SYSTEM_THREAD_EXCEPTION_NOT_HANDLED_M', 'driver')
    '1000007F' = @('UNEXPECTED_KERNEL_MODE_TRAP_M', 'hardware')
    '1000008E' = @('KERNEL_MODE_EXCEPTION_NOT_HANDLED_M', 'driver')
    '100000EA' = @('THREAD_STUCK_IN_DEVICE_DRIVER_M', 'gpu')
    'C000021A' = @('WINLOGON_FATAL_ERROR', 'system')
    'C0000221' = @('STATUS_IMAGE_CHECKSUM_MISMATCH', 'system')
    'DEADDEAD' = @('MANUALLY_INITIATED_CRASH1', 'manual')
}

# Programs of Windows Update that restart the PC to finish installing updates
# (file names, lower case).
$updatePrograms = @(
    'mousocoreworker.exe', 'usoclient.exe', 'musnotification.exe', 'musnotificationux.exe',
    'trustedinstaller.exe', 'wuauclt.exe', 'setuphost.exe'
)

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return (([string]$Value) -replace '[\x00-\x1f]', ' ').Trim()
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

# A number written in decimal or as 0x..., or -1.
function ConvertTo-Code {
    param([string]$Text)
    $number = [uint64]0
    if ($Text -match '^0[xX]([0-9A-Fa-f]{1,16})$') {
        $number = [Convert]::ToUInt64($Matches[1], 16)
    }
    elseif (-not [uint64]::TryParse($Text, [Globalization.NumberStyles]::None, $invariant, [ref]$number)) {
        return [int64]-1
    }
    if ($number -gt [uint32]::MaxValue) {
        return [int64]-1
    }
    return [int64]$number
}

# The stop code of a 1001 event: the value that starts with it, or -1.
function Get-WerCode {
    param($Record)
    foreach ($property in @($Record.Properties)) {
        $m = [regex]::Match((Get-Text $property.Value), '^0[xX]([0-9A-Fa-f]{1,16})\b')
        if ($m.Success) {
            return (ConvertTo-Code ('0x' + $m.Groups[1].Value))
        }
    }
    return [int64]-1
}

# A bug check parameter (0x... or decimal, up to 64 bits), or $null.
function ConvertTo-Parameter {
    param([string]$Text)
    $t = Get-Text $Text
    if ($t -match '^0[xX]([0-9A-Fa-f]{1,16})$') {
        return [Convert]::ToUInt64($Matches[1], 16)
    }
    $number = [uint64]0
    if ([uint64]::TryParse($t, [Globalization.NumberStyles]::None, $invariant, [ref]$number)) {
        return $number
    }
    return $null
}

# The four parameters, or an empty list when one of them cannot be read.
function ConvertTo-ParameterList {
    param([string[]]$Texts)
    $list = New-Object System.Collections.Generic.List[uint64]
    foreach ($text in $Texts) {
        $value = ConvertTo-Parameter $text
        if ($null -eq $value) {
            return , @()
        }
        $list.Add($value)
    }
    if ($list.Count -ne 4) {
        return , @()
    }
    return , $list.ToArray()
}

# The parameters of a 1001 event: "0x0000007a (0x4, 0xffffffffc000009c, ...)".
function Get-WerParameters {
    param($Record)
    foreach ($property in @($Record.Properties)) {
        $m = [regex]::Match((Get-Text $property.Value), '^0[xX][0-9A-Fa-f]{1,16}\s*\(([^)]*)\)')
        if ($m.Success) {
            return , (ConvertTo-ParameterList $m.Groups[1].Value.Split(','))
        }
    }
    return , @()
}

# What an I/O status (the low 32 bits of a parameter; on 64-bit Windows it is
# sign-extended, 0xffffffffc000009c) says, or ''. Formatted as text, so no
# 64-bit arithmetic is needed.
function Get-StatusDetail {
    param([uint64]$Value)
    $status = ('{0:X16}' -f $Value).Substring(8)
    if (($status -eq 'C000009C') -or ($status -eq 'C000016A')) {
        return 'disk-bad-sector'
    }
    if (($status -eq 'C000009D') -or ($status -eq 'C0000185') -or ($status -eq 'C000000E')) {
        return 'disk-cable'
    }
    if ($status -eq 'C000009A') {
        return 'low-resources'
    }
    return ''
}

# What the parameters of a blue screen say (Microsoft's pages for 0x7A, 0x77,
# 0xA0 and 0xFE), or ''.
function Get-Detail {
    param([int64]$Code, [uint64[]]$Parameters)
    if (@($Parameters).Count -ne 4) {
        return ''
    }
    $p1 = $Parameters[0]
    $p2 = $Parameters[1]
    $p3 = $Parameters[2]
    if ($Code -eq 0x7A) {
        return (Get-StatusDetail $p2)
    }
    if (($Code -eq 0x77) -and ($p1 -gt 2)) {
        return (Get-StatusDetail $p2)
    }
    if (($Code -eq 0xA0) -and ($p1 -eq 11)) {
        return 'hiberfile-small'
    }
    if ($Code -eq 0xFE) {
        if (($p1 -eq 3) -or ($p1 -eq 5)) {
            return 'usb-hardware'
        }
        if (($p1 -eq 8) -and ($p2 -eq 6) -and ($p3 -le 9) -and (@(1, 2, 3, 4, 5, 8, 9) -contains [int]$p3)) {
            return 'usb-suspend'
        }
    }
    return ''
}

function Format-Time {
    param([datetime]$Time)
    return $Time.ToString('yyyy-MM-dd HH:mm', $invariant)
}

$events = @(Get-SystemEvent @{ LogName = 'System'; Id = 41, 1001, 1074; StartTime = $since })

$crashes = New-Object System.Collections.Generic.List[object]
$reports = New-Object System.Collections.Generic.List[object]
$shutdowns = 0
$powerButton = 0
$lastShutdown = $null
$updates = 0
$lastUpdate = $null
foreach ($record in $events) {
    $provider = [string]$record.ProviderName
    $id = [int]$record.Id
    $time = $record.TimeCreated
    if ($null -eq $time) {
        continue
    }
    if (($id -eq 1001) -and (($provider -eq 'Microsoft-Windows-WER-SystemErrorReporting') -or ($provider -eq 'BugCheck'))) {
        $code = Get-WerCode $record
        $reports.Add([pscustomobject]@{ Time = $time; Code = $code; Paired = $false; Detail = (Get-Detail $code (Get-WerParameters $record)) })
    }
    elseif (($id -eq 41) -and ($provider -eq 'Microsoft-Windows-Kernel-Power')) {
        $values = Get-EventValue $record
        $code = ConvertTo-Code ([string]$values['BugcheckCode'])
        if ($code -gt 0) {
            $parameters = ConvertTo-ParameterList @(1..4 | ForEach-Object { [string]$values['BugcheckParameter' + $_] })
            $crashes.Add([pscustomobject]@{ Time = $time; Code = $code; Detail = (Get-Detail $code $parameters) })
            continue
        }
        $shutdowns++
        $stamp = [string]$values['PowerButtonTimestamp']
        if ((($stamp.Length -gt 0) -and ($stamp -ne '0')) -or ([string]$values['LongPowerButtonPressDetected'] -ieq 'true')) {
            $powerButton++
        }
        if (($null -eq $lastShutdown) -or ($time -gt $lastShutdown)) {
            $lastShutdown = $time
        }
    }
    elseif (($id -eq 1074) -and ($provider -eq 'User32')) {
        $values = Get-EventValue $record
        $m = [regex]::Match([string]$values['param1'], '(?i)([^\\/\s(]+\.exe)\b')
        if ($m.Success -and ($updatePrograms -contains $m.Groups[1].Value.ToLowerInvariant())) {
            $updates++
            if (($null -eq $lastUpdate) -or ($time -gt $lastUpdate)) {
                $lastUpdate = $time
            }
        }
    }
}

# One blue screen: its 1001 (the dump was saved) and / or its 41 (the code was
# recorded). A 41 pairs with the nearest unpaired 1001 of the same code logged
# from 2 minutes before to $pairMinutes minutes after it.
$blueScreens = New-Object System.Collections.Generic.List[object]
foreach ($crash in $crashes) {
    $match = $null
    foreach ($report in $reports) {
        if ($report.Paired -or ($report.Code -ne $crash.Code)) {
            continue
        }
        $minutes = ($report.Time - $crash.Time).TotalMinutes
        if (($minutes -lt -2) -or ($minutes -gt $pairMinutes)) {
            continue
        }
        if (($null -eq $match) -or ([math]::Abs($minutes) -lt [math]::Abs(($match.Time - $crash.Time).TotalMinutes))) {
            $match = $report
        }
    }
    if ($null -ne $match) {
        $match.Paired = $true
        if ([string]$match.Detail -eq '') {
            $match.Detail = $crash.Detail
        }
    }
    else {
        $blueScreens.Add($crash)
    }
}
foreach ($report in $reports) {
    $blueScreens.Add([pscustomobject]@{ Time = $report.Time; Code = $report.Code; Detail = $report.Detail })
}

# Crash dumps since the last start. Unknown (not reported) when the start time
# cannot be read.
$dumpOff = $false
try {
    $lastBoot = (Get-CimInstance -ClassName Win32_OperatingSystem -Property LastBootUpTime -ErrorAction Stop).LastBootUpTime
    if ($lastBoot -is [datetime]) {
        $failed = @(Get-SystemEvent @{ LogName = 'System'; Id = 46; StartTime = $lastBoot.AddMinutes(-5) } |
                Where-Object { $_.ProviderName -eq 'volmgr' })
        $dumpOff = $failed.Count -gt 0
    }
}
catch {
    $dumpOff = $false
}

# ---- by stop code, the newest first
$groups = @{}
foreach ($screen in $blueScreens) {
    $key = ''
    if ($screen.Code -ge 0) {
        $key = '{0:X8}' -f $screen.Code
    }
    if (-not $groups.ContainsKey($key)) {
        $groups[$key] = [pscustomobject]@{ Key = $key; Times = 0; First = $screen.Time; Last = $screen.Time; Detail = ''; DetailTime = $null }
    }
    $group = $groups[$key]
    $group.Times++
    if (([string]$screen.Detail -ne '') -and (($null -eq $group.DetailTime) -or ($screen.Time -gt $group.DetailTime))) {
        $group.Detail = [string]$screen.Detail
        $group.DetailTime = $screen.Time
    }
    if ($screen.Time -lt $group.First) {
        $group.First = $screen.Time
    }
    if ($screen.Time -gt $group.Last) {
        $group.Last = $screen.Time
    }
}

$sections = New-Object System.Collections.Generic.List[object]
foreach ($group in @($groups.Values | Sort-Object -Property Last -Descending | Select-Object -First $maxCodes)) {
    $rows = New-Object System.Collections.Generic.List[object]
    $rows.Add([ordered]@{ id = 'times'; value = $group.Times })
    if ($group.Times -gt 1) {
        $rows.Add([ordered]@{ id = 'first'; value = (Format-Time $group.First) })
    }
    $rows.Add([ordered]@{ id = 'last'; value = (Format-Time $group.Last) })
    $name = ''
    $cause = 'unknown'
    $known = $null
    if ($group.Key.Length -gt 0) {
        $known = $stops[$group.Key]
    }
    if ($null -ne $known) {
        $name = $known[0]
        $cause = $known[1]
        $rows.Add([ordered]@{ id = 'code'; value = ('0x' + $group.Key) })
    }
    elseif ($group.Key.Length -gt 0) {
        $name = '0x' + $group.Key
    }
    $rows.Add([ordered]@{ id = 'cause'; code = $cause })
    if ($group.Detail.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'detail'; code = $group.Detail })
    }
    $rows.Add([ordered]@{ id = 'advice'; code = ($cause + '-advice') })
    $section = [ordered]@{ id = 'bluescreen' }
    if ($name.Length -gt 0) {
        $section['name'] = $name
    }
    $section['rows'] = $rows.ToArray()
    $sections.Add($section)
}

if ($shutdowns -gt 0) {
    $rows = New-Object System.Collections.Generic.List[object]
    $rows.Add([ordered]@{ id = 'times'; value = $shutdowns })
    if ($powerButton -gt 0) {
        $rows.Add([ordered]@{ id = 'power_button'; value = $powerButton })
    }
    $rows.Add([ordered]@{ id = 'last'; value = (Format-Time $lastShutdown) })
    $rows.Add([ordered]@{ id = 'advice'; code = 'shutdown-advice' })
    $sections.Add([ordered]@{ id = 'shutdowns'; rows = $rows.ToArray() })
}

if ($updates -gt 0) {
    $rows = New-Object System.Collections.Generic.List[object]
    $rows.Add([ordered]@{ id = 'times'; value = $updates })
    $rows.Add([ordered]@{ id = 'last'; value = (Format-Time $lastUpdate) })
    $rows.Add([ordered]@{ id = 'advice'; code = 'update-advice' })
    $sections.Add([ordered]@{ id = 'updates'; rows = $rows.ToArray() })
}

if ($dumpOff) {
    $sections.Add([ordered]@{
            id   = 'dump'
            rows = @(
                [ordered]@{ id = 'state'; code = 'dump-off' },
                [ordered]@{ id = 'advice'; code = 'dump-advice' }
            )
        })
}

$result = 'none'
if ($blueScreens.Count -gt 0) {
    $result = 'listed'
}
elseif ($shutdowns -gt 0) {
    $result = 'shutdowns'
}
elseif ($dumpOff) {
    $result = 'no-dump'
}
elseif ($updates -gt 0) {
    $result = 'updates'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{
        days         = $days
        count        = $blueScreens.Count
        codes        = $groups.Count
        shutdowns    = $shutdowns
        power_button = $powerButton
        updates      = $updates
        dump_off     = $dumpOff
    }
    sections = $sections.ToArray()
}
