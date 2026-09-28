# Check: network.hotspot-services
# Are the services the mobile hotspot needs switched off? "Optimizer" tools
# and "turn off useless services" guides often disable them. Their Windows
# defaults (Microsoft, "Guidance on disabling system services on Windows
# Server with Desktop Experience"):
# - Windows Mobile Hotspot Service (icssvc): Manual, "Provides the ability to
#   share a cellular data connection with another device"; installed only
#   with the desktop experience.
# - Internet Connection Sharing (SharedAccess): Manual, "Required for clients
#   used as WiFi hotspots and both ends of Miracast projection".
# Manual is fine (Windows starts them when the hotspot is turned on);
# disabled is not: "We can't set up mobile hotspot".
# Read-only. Result codes, in this order:
#   missing   one of the two is not there (a stripped-down Windows, or
#             Windows Server without the desktop experience)
#   disabled  one of them is disabled
#   ok
# disabled is fixed by network.enable-hotspot-services.
# Facts: icssvc, sharedaccess (their Start values, '' when missing).

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

$hotspot = Get-StartValue 'icssvc'
$sharing = Get-StartValue 'SharedAccess'
$facts = [ordered]@{ icssvc = ''; sharedaccess = '' }
if ($null -ne $hotspot) {
    $facts.icssvc = $hotspot
}
if ($null -ne $sharing) {
    $facts.sharedaccess = $sharing
}

$result = 'ok'
if (($null -eq $hotspot) -or ($null -eq $sharing)) {
    $result = 'missing'
}
elseif (($hotspot -eq 4) -or ($sharing -eq 4)) {
    $result = 'disabled'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
