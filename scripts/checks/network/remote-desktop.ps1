# Check: network.remote-desktop
# Can other computers connect to this one with Remote Desktop? Read-only: it
# only looks, and turning Remote Desktop on is left to Settings > System >
# Remote Desktop (the same switch also opens the firewall for it).
# What is looked at follows Microsoft's "General Remote Desktop connection
# troubleshooting" and "Remote Desktop can't connect to the remote computer"
# (compared with the list in paulmann/RDP-Diagnostic-Tool, MIT):
# - Edition: HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion EditionID.
#   Core* is Home, and "Windows Home editions can't be used as a Remote
#   Desktop host" ("Enable Remote Desktop on your PC").
# - Policy: HKLM\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services
#   fDenyTSConnections ("Allow users to connect remotely by using Remote
#   Desktop Services": 1 = Disabled, 0 = Enabled). A policy wins over the
#   setting.
# - Setting: HKLM\SYSTEM\CurrentControlSet\Control\Terminal Server
#   fDenyTSConnections (0 = on, 1 = off).
# - Service: Remote Desktop Services (TermService), Start 4 = disabled.
# - Listener: ...\Terminal Server\WinStations\RDP-Tcp fEnableWinStation
#   (must be 1) and PortNumber (3389; any other port has to be typed after the
#   computer name as name:port).
# - Firewall: inbound rules of the "Remote Desktop" group, found by the group's
#   language-independent name @FirewallAPI.dll,-28752 (Microsoft, unattend
#   FirewallGroup). Turning Remote Desktop on in Settings enables them.
# Nothing about the computer's name, addresses or accounts is read.
# Result codes, in this order:
#   home             Home edition: other computers can't connect to it
#   policy-off       a policy turns Remote Desktop off (work computers)
#   missing          the Remote Desktop Services service is not there
#   service-disabled the Remote Desktop Services service is disabled
#   off              Remote Desktop is off
#   listener-off     on, but the RDP-Tcp listener is switched off
#   firewall-closed  on, but every inbound "Remote Desktop" firewall rule is off
#   custom-port      on, listening on another port than 3389
#   on               on
# Facts: port, service (TermService Start value, '' when missing),
# firewall (open / closed / unknown).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$tsKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server'
$rdpKey = $tsKey + '\WinStations\RDP-Tcp'
$policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services'

# A registry value, or $null when the key or the value is not there.
function Get-Value {
    param([string]$Path, [string]$Name)
    try {
        return (Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop).$Name
    }
    catch {
        return $null
    }
}

# open: at least one inbound rule of the group is enabled; closed: none is;
# unknown: the rules can't be read (no NetSecurity module, no group).
function Get-FirewallState {
    try {
        $rules = @(Get-NetFirewallRule -Group '@FirewallAPI.dll,-28752' -ErrorAction Stop |
                Where-Object { [string]$_.Direction -eq 'Inbound' })
    }
    catch {
        return 'unknown'
    }
    if ($rules.Count -eq 0) {
        return 'unknown'
    }
    foreach ($rule in $rules) {
        if ([string]$rule.Enabled -eq 'True') {
            return 'open'
        }
    }
    return 'closed'
}

$edition = [string](Get-Value 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' 'EditionID')
$policy = Get-Value $policyKey 'fDenyTSConnections'
$setting = Get-Value $tsKey 'fDenyTSConnections'
$service = Get-Value 'HKLM:\SYSTEM\CurrentControlSet\Services\TermService' 'Start'
$listener = Get-Value $rdpKey 'fEnableWinStation'
$port = Get-Value $rdpKey 'PortNumber'
if ($null -eq $port) {
    $port = 3389
}

$facts = [ordered]@{ port = [int]$port; service = ''; firewall = 'unknown' }
if ($null -ne $service) {
    $facts.service = [int]$service
}

# The policy decides when it is there; otherwise the setting (missing counts
# as off, which is how Windows ships).
if ($null -ne $policy) {
    $enabled = ([int]$policy -eq 0)
}
else {
    $enabled = (($null -ne $setting) -and ([int]$setting -eq 0))
}

$result = 'on'
if ($edition -like 'Core*') {
    $result = 'home'
}
elseif (($null -ne $policy) -and ([int]$policy -ne 0)) {
    $result = 'policy-off'
}
elseif ($null -eq $service) {
    $result = 'missing'
}
elseif ([int]$service -eq 4) {
    $result = 'service-disabled'
}
elseif (-not $enabled) {
    $result = 'off'
}
elseif (($null -ne $listener) -and ([int]$listener -ne 1)) {
    $result = 'listener-off'
}
else {
    $facts.firewall = Get-FirewallState
    if ($facts.firewall -eq 'closed') {
        $result = 'firewall-closed'
    }
    elseif ([int]$port -ne 3389) {
        $result = 'custom-port'
    }
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
