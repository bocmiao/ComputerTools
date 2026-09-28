# Check: network.wireless-services
# Are the services WiFi and airplane mode need switched off? "Optimizer"
# tools and tuning guides often do this. Their Windows defaults:
# - WLAN AutoConfig (WlanSvc): Automatic. It finds and connects to wireless
#   networks; while it is not running, the network list has no Wi-Fi heading
#   and no wireless network is found (Microsoft, "Wireless LAN Service
#   Overview"). Not installed on Windows Server unless the Wireless LAN
#   Service feature is added.
# - Windows Connection Manager (Wcmsvc): Automatic (trigger start). WLAN
#   AutoConfig depends on it and cannot start while it is disabled.
# - Radio Management Service (RmSvc): Manual, "Radio Management and Airplane
#   Mode Service" (Microsoft, "Guidance on disabling system services on
#   Windows Server"). While it is disabled the airplane mode button does
#   nothing or is greyed out.
# Read-only. Result codes, in this order:
#   missing          one of the three is not there (Windows Server, or a
#                    stripped-down Windows)
#   disabled         WlanSvc or Wcmsvc is disabled
#   manual           WlanSvc is set to Manual and is not running
#   radio-disabled   only RmSvc is disabled
#   ok
# All but missing are fixed by network.enable-wireless-services, which puts
# the three back to their defaults.
# Facts: wlansvc, wcmsvc, rmsvc (their Start values, '' when missing),
# wlansvc_running.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# A service's Start value, or $null when the service is not there.
function Get-StartValue {
    param([string]$Name)
    try {
        return [int](Get-ItemProperty -LiteralPath ('HKLM:\SYSTEM\CurrentControlSet\Services\' + $Name) -Name 'Start' -ErrorAction Stop).Start
    }
    catch {
        return $null
    }
}

# $true when the service is running, $false when it is not, $null when it
# cannot be read.
function Test-ServiceRunning {
    param([string]$Name)
    try {
        $service = Get-Service -Name $Name -ErrorAction Stop
        return ([string]$service.Status -eq 'Running')
    }
    catch {
        return $null
    }
}

$wlan = Get-StartValue 'WlanSvc'
$wcm = Get-StartValue 'Wcmsvc'
$radio = Get-StartValue 'RmSvc'
$facts = [ordered]@{ wlansvc = ''; wcmsvc = ''; rmsvc = ''; wlansvc_running = '' }
foreach ($pair in @(@('wlansvc', $wlan), @('wcmsvc', $wcm), @('rmsvc', $radio))) {
    if ($null -ne $pair[1]) {
        $facts[$pair[0]] = $pair[1]
    }
}

$result = 'ok'
if (($null -eq $wlan) -or ($null -eq $wcm) -or ($null -eq $radio)) {
    $result = 'missing'
}
elseif (($wlan -eq 4) -or ($wcm -eq 4)) {
    $result = 'disabled'
}
elseif ($wlan -eq 3) {
    $running = Test-ServiceRunning 'WlanSvc'
    if ($null -ne $running) {
        $facts.wlansvc_running = $running
    }
    # Running now: WiFi works, nothing to report. Not running, or unknown:
    # nothing starts it, so there is no WiFi.
    if ($running -ne $true) {
        $result = 'manual'
    }
}
if (($result -eq 'ok') -and ($radio -eq 4)) {
    $result = 'radio-disabled'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
