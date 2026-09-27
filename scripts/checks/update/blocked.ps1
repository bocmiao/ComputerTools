# Check: update.blocked
# Has Windows Update been turned off? "Optimizer" tools and online guides turn
# it off in a few ways; this check reads each of them. Read-only.
# Services (the registry key the service manager saves, Win32_Service):
#   the update services (see the shared block) set to Disabled, or missing;
#   Windows Update (wuauserv) set to log on as some account other than Local
#   System (a guide trick: the service then cannot start, and Windows Update
#   Medic cannot repair it). The account name is not reported.
# Policies (HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate and \AU),
# read only: medkit never writes there (plan section 5, item 5):
#   SetDisableUXWUAccess = 1          no access to Windows Update in Settings
#   UseWUServer = 1 with WUServer     updates come from an update server of an
#                                     organization; on a home PC it usually
#                                     points nowhere
#   NoAutoUpdate = 1                  no automatic updates (checking by hand
#                                     still works)
# Pause (HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings):
#   PauseUpdatesExpiryTime, PauseFeatureUpdatesEndTime,
#   PauseQualityUpdatesEndTime        when a pause ends (UTC, ISO 8601)
#   FlightSettingsMaxPauseDays        a raised limit for pausing in Settings
# Result codes, in this order (what medkit can fix first):
#   disabled       an update service is Disabled (fix: update.enable-services)
#   missing        an update service does not exist (a stripped-down Windows)
#   logon          Windows Update logs on as another account
#   medic-disabled only Windows Update Medic is Disabled (the fix cannot
#                  change it)
#   no-access      SetDisableUXWUAccess
#   update-server  updates come from an update server (UseWUServer, WUServer)
#   paused-long    paused for longer than Settings allows ($maxPauseDays)
#   auto-off       NoAutoUpdate
#   paused         paused within the normal limit
#   ok
# Facts: disabled_services, missing_services (display names), no_access,
# update_server, auto_off (true/false), paused_until (local date or ''),
# pause_days (days left), pause_limit_days (FlightSettingsMaxPauseDays or 0).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block update-services: identical in checks/update/blocked.ps1 and features/update/enable-services-*.ps1 (medkit-data check compares them) ----
# The services Windows Update needs, with the start type Windows gives each of
# them (the names sc.exe uses: auto, delayed-auto, demand). "Optimizer" tools
# set them to Disabled. Windows Update Medic (WaaSMedicSvc) is checked as well,
# but Windows does not let administrators change it: it has no start type
# here, and the fix leaves it alone.
$updateServiceDefaults = [ordered]@{
    'wuauserv'         = 'demand'
    'UsoSvc'           = 'delayed-auto'
    'BITS'             = 'demand'
    'CryptSvc'         = 'auto'
    'TrustedInstaller' = 'demand'
    'DoSvc'            = 'delayed-auto'
    'WaaSMedicSvc'     = ''
}

# How a service starts, as the service manager saved it in the registry:
# auto, delayed-auto, demand, disabled, other, or missing.
function Get-ServiceStart {
    param([string]$Name)
    $key = 'HKLM:\SYSTEM\CurrentControlSet\Services\' + $Name
    if (-not (Test-Path -LiteralPath $key)) {
        return 'missing'
    }
    $properties = Get-ItemProperty -LiteralPath $key
    $start = $properties.PSObject.Properties['Start']
    if ($null -eq $start) {
        return 'other'
    }
    switch ([int64]$start.Value) {
        2 {
            $delayed = $properties.PSObject.Properties['DelayedAutostart']
            if (($null -ne $delayed) -and ([int64]$delayed.Value -eq 1)) {
                return 'delayed-auto'
            }
            return 'auto'
        }
        3 {
            return 'demand'
        }
        4 {
            return 'disabled'
        }
        default {
            return 'other'
        }
    }
}
# ---- end of shared block update-services ----

# Settings allows pausing for up to 5 weeks; a day more for time zones.
$maxPauseDays = 36

function Get-Value {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $property = (Get-ItemProperty -LiteralPath $Path).PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function Test-One {
    param($Value)
    if ($null -eq $Value) {
        return $false
    }
    try {
        return ([int64]$Value -eq 1)
    }
    catch {
        return $false
    }
}

# Services. Display names are what the Services window shows.
$disabled = New-Object System.Collections.Generic.List[string]
$missing = New-Object System.Collections.Generic.List[string]
$medicDisabled = $false
foreach ($name in $updateServiceDefaults.Keys) {
    $start = Get-ServiceStart $name
    if ($start -eq 'missing') {
        $missing.Add($name)
    }
    elseif ($start -eq 'disabled') {
        if ($updateServiceDefaults[$name].Length -eq 0) {
            $medicDisabled = $true
            continue
        }
        $service = Get-Service -Name $name -ErrorAction SilentlyContinue
        if ($null -ne $service) {
            $disabled.Add([string]$service.DisplayName)
        }
        else {
            $disabled.Add($name)
        }
    }
}
$logonChanged = $false
$wu = Get-CimInstance -ClassName Win32_Service -Filter "Name = 'wuauserv'" -Property StartName
if (($null -ne $wu) -and ([string]$wu.StartName).Length -gt 0) {
    $logonChanged = ([string]$wu.StartName -ne 'LocalSystem')
}

# Policies
$policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate'
$auKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU'
$noAccess = Test-One (Get-Value $policyKey 'SetDisableUXWUAccess')
$server = [string](Get-Value $policyKey 'WUServer')
$updateServer = (Test-One (Get-Value $auKey 'UseWUServer')) -and ($server.Trim().Length -gt 0)
$autoOff = Test-One (Get-Value $auKey 'NoAutoUpdate')

# Pause
$uxKey = 'HKLM:\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings'
$now = [DateTime]::UtcNow
$pauseEnd = $null
foreach ($name in @('PauseUpdatesExpiryTime', 'PauseFeatureUpdatesEndTime', 'PauseQualityUpdatesEndTime')) {
    $text = [string](Get-Value $uxKey $name)
    if ($text.Length -eq 0) {
        continue
    }
    $end = [DateTime]::MinValue
    $styles = [System.Globalization.DateTimeStyles]::AdjustToUniversal -bor [System.Globalization.DateTimeStyles]::AssumeUniversal
    if ([DateTime]::TryParse($text, [System.Globalization.CultureInfo]::InvariantCulture, $styles, [ref]$end)) {
        if (($end -gt $now) -and (($null -eq $pauseEnd) -or ($end -gt $pauseEnd))) {
            $pauseEnd = $end
        }
    }
}
$pauseLimit = 0
$limit = Get-Value $uxKey 'FlightSettingsMaxPauseDays'
if ($null -ne $limit) {
    try {
        $pauseLimit = [int64]$limit
    }
    catch {
        $pauseLimit = 0
    }
}
$pausedUntil = ''
$pauseDays = 0
if ($null -ne $pauseEnd) {
    $pausedUntil = $pauseEnd.ToLocalTime().ToString('yyyy-MM-dd')
    $pauseDays = [int][math]::Ceiling(($pauseEnd - $now).TotalDays)
}

$result = 'ok'
if ($disabled.Count -gt 0) {
    $result = 'disabled'
}
elseif ($missing.Count -gt 0) {
    $result = 'missing'
}
elseif ($logonChanged) {
    $result = 'logon'
}
elseif ($medicDisabled) {
    $result = 'medic-disabled'
}
elseif ($noAccess) {
    $result = 'no-access'
}
elseif ($updateServer) {
    $result = 'update-server'
}
elseif ($pauseDays -gt $maxPauseDays) {
    $result = 'paused-long'
}
elseif ($autoOff) {
    $result = 'auto-off'
}
elseif ($pauseDays -gt 0) {
    $result = 'paused'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        disabled_services = $disabled.ToArray()
        missing_services  = $missing.ToArray()
        medic_disabled    = $medicDisabled
        logon_changed     = $logonChanged
        no_access         = $noAccess
        update_server     = $updateServer
        auto_off          = $autoOff
        paused_until      = $pausedUntil
        pause_days        = $pauseDays
        pause_limit_days  = $pauseLimit
    }
}
