# Check: network.wireless-services
# Are the services WiFi and airplane mode need switched off? "Optimizer"
# tools and tuning guides often do this. Their Windows defaults:
# - WLAN AutoConfig (WlanSvc): Automatic on Windows 11, Manual on most
#   Windows 10 versions (started when a WiFi adapter needs it) and on Windows
#   Server 2025. It finds and connects to wireless networks; while it is not
#   running, the network list has no Wi-Fi heading and no wireless network is
#   found (Microsoft, "Wireless LAN Service Overview"). Manual is therefore
#   not a problem; disabled is.
# - Windows Connection Manager (Wcmsvc): Automatic (trigger start). WLAN
#   AutoConfig depends on it and cannot start while it is disabled.
# - Radio Management Service (RmSvc): Manual, "Radio Management and Airplane
#   Mode Service" (Microsoft, "Guidance on disabling system services on
#   Windows Server"). While it is disabled the airplane mode button does
#   nothing or is greyed out.
# Read-only. Result codes, in this order:
#   missing          one of the three is not there (Windows Server without
#                    the Wireless LAN Service feature, or a stripped-down
#                    Windows)
#   disabled         WlanSvc or Wcmsvc is disabled
#   radio-disabled   only RmSvc is disabled
#   ok
# disabled and radio-disabled are fixed by network.enable-wireless-services.
# Facts: wlansvc, wcmsvc, rmsvc (their Start values, '' when missing).

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

$wlan = Get-StartValue 'WlanSvc'
$wcm = Get-StartValue 'Wcmsvc'
$radio = Get-StartValue 'RmSvc'
$facts = [ordered]@{ wlansvc = ''; wcmsvc = ''; rmsvc = '' }
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
elseif ($radio -eq 4) {
    $result = 'radio-disabled'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
