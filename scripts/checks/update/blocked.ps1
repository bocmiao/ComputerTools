# Check: update.blocked
# Has Windows Update been turned off? "Optimizer" tools and online guides turn
# it off in a few ways; this check reads each of them. Read-only.
# Services (the registry key the service manager saves, Win32_Service):
#   the update services (see the shared block) set to Disabled, or missing;
#   Windows Update (wuauserv) set to log on as some account other than Local
#   System (a guide trick: the service then cannot start, and Windows Update
#   Medic cannot repair it). The account name is not reported.
# Policies, read only: medkit never writes them (plan section 5, item 5).
# Microsoft documents them for Pro and up; Home is not covered by its docs.
#   HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate:
#     DisableWindowsUpdateAccess = 1  "Turn off access to all Windows Update
#                                     features": scans fail (0x8024002F)
#     SetDisableUXWUAccess = 1        "Remove access to use all Windows Update
#                                     features": no "Check for updates" in
#                                     Settings; updates in the background go on
#   ...\WindowsUpdate\AU:
#     UseWUServer = 1                 updates come from the update server of an
#                                     organization (WUServer); on a home PC it
#                                     usually points nowhere, or is missing
#     NoAutoUpdate = 1                no automatic updates (checking by hand
#                                     still works, unless SetDisableUXWUAccess)
#   HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer:
#     SettingsPageVisibility          hides the Windows Update page of Settings
#                                     ("hide:...windowsupdate..." or a
#                                     "showonly:" list without it)
# Pause (HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings, what Settings
# writes; the values stay after a pause ends, so the end time decides):
#   PauseUpdatesExpiryTime, PauseFeatureUpdatesEndTime,
#   PauseQualityUpdatesEndTime        when a pause ends (UTC, ISO 8601)
#   FlightSettingsMaxPauseDays        a raised limit for pausing in Settings
# Result codes, in this order (what medkit can fix first, then what blocks
# updates completely):
#   disabled            an update service is Disabled (fix: update.enable-services)
#   missing             an update service does not exist (a stripped-down Windows)
#   logon               Windows Update logs on as another account
#   protected-disabled  a protected update service is Disabled (the fix
#                       cannot change it)
#   update-server       UseWUServer
#   access-off          DisableWindowsUpdateAccess
#   paused-long         paused for longer than Settings allows ($maxPauseDays)
#   all-off             NoAutoUpdate and SetDisableUXWUAccess together
#   auto-off            NoAutoUpdate
#   no-check            SetDisableUXWUAccess
#   page-hidden         SettingsPageVisibility hides the page
#   paused              paused within the normal limit
#   ok
# Facts: disabled_services, missing_services, protected_disabled (names as the
# Services window shows them), logon_changed, update_server, access_off,
# no_check, auto_off, page_hidden (true/false), paused_until (local date or
# ''), pause_days (days left), pause_limit_days (FlightSettingsMaxPauseDays
# or 0).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block update-services: identical in checks/update/blocked.ps1 and features/update/enable-services-*.ps1 (medkit-data check compares them) ----
# The services Windows Update needs, with the start type Windows gives each of
# them (the names sc.exe uses: auto, delayed-auto, demand). "Optimizer" tools
# set them to Disabled. BITS switches itself between demand and delayed-auto,
# and Windows Modules Installer (TrustedInstaller) to auto while an update
# waits for a restart; both are fine. Windows Update Medic (WaaSMedicSvc) and
# Delivery Optimization (DoSvc) are protected services: the service manager
# refuses to change them even for administrators, so they have no start type
# here, and the fix leaves them alone (the check reports them).
$updateServiceDefaults = [ordered]@{
    'wuauserv'         = 'demand'
    'UsoSvc'           = 'delayed-auto'
    'BITS'             = 'demand'
    'CryptSvc'         = 'auto'
    'TrustedInstaller' = 'demand'
    'DoSvc'            = ''
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

# The name the Services window shows, or the service name when Windows cannot
# resolve it (Windows Update Medic often shows "@WaaSMedicSvcImpl.dll,-100").
function Get-ServiceLabel {
    param([string]$Name)
    $service = Get-Service -Name $Name -ErrorAction SilentlyContinue
    if ($null -ne $service) {
        $label = [string]$service.DisplayName
        if (($label.Length -gt 0) -and (-not $label.StartsWith('@'))) {
            return $label
        }
    }
    return $Name
}

# Services
$disabled = New-Object System.Collections.Generic.List[string]
$missing = New-Object System.Collections.Generic.List[string]
$protected = New-Object System.Collections.Generic.List[string]
foreach ($name in $updateServiceDefaults.Keys) {
    $start = Get-ServiceStart $name
    if ($start -eq 'missing') {
        $missing.Add($name)
    }
    elseif (($start -eq 'disabled') -and ($updateServiceDefaults[$name].Length -gt 0)) {
        $disabled.Add((Get-ServiceLabel $name))
    }
    elseif ($start -eq 'disabled') {
        $protected.Add((Get-ServiceLabel $name))
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
$accessOff = Test-One (Get-Value $policyKey 'DisableWindowsUpdateAccess')
$noCheck = Test-One (Get-Value $policyKey 'SetDisableUXWUAccess')
$updateServer = Test-One (Get-Value $auKey 'UseWUServer')
$autoOff = Test-One (Get-Value $auKey 'NoAutoUpdate')
$pageHidden = $false
$visibility = ([string](Get-Value 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer' 'SettingsPageVisibility')).Trim().ToLowerInvariant()
if ($visibility.Contains(':')) {
    $mode = $visibility.Substring(0, $visibility.IndexOf(':'))
    $pages = @($visibility.Substring($visibility.IndexOf(':') + 1).Split(';') | ForEach-Object { $_.Trim() })
    $listed = ($pages -contains 'windowsupdate')
    $pageHidden = (($mode -eq 'hide') -and $listed) -or (($mode -eq 'showonly') -and (-not $listed))
}

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
elseif ($protected.Count -gt 0) {
    $result = 'protected-disabled'
}
elseif ($updateServer) {
    $result = 'update-server'
}
elseif ($accessOff) {
    $result = 'access-off'
}
elseif ($pauseDays -gt $maxPauseDays) {
    $result = 'paused-long'
}
elseif ($autoOff -and $noCheck) {
    $result = 'all-off'
}
elseif ($autoOff) {
    $result = 'auto-off'
}
elseif ($noCheck) {
    $result = 'no-check'
}
elseif ($pageHidden) {
    $result = 'page-hidden'
}
elseif ($pauseDays -gt 0) {
    $result = 'paused'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        disabled_services  = $disabled.ToArray()
        missing_services   = $missing.ToArray()
        protected_disabled = $protected.ToArray()
        logon_changed      = $logonChanged
        update_server      = $updateServer
        access_off         = $accessOff
        no_check           = $noCheck
        auto_off           = $autoOff
        page_hidden        = $pageHidden
        paused_until       = $pausedUntil
        pause_days         = $pauseDays
        pause_limit_days   = $pauseLimit
    }
}
