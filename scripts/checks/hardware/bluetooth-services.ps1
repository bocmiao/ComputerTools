# Check: hardware.bluetooth-services
# Are the Bluetooth services disabled? "Optimizer" tools often do this. Both
# are Manual (trigger start) in Windows (Microsoft, "Guidance on disabling
# system services on Windows Server"):
# - Bluetooth Support Service (bthserv): without it no Bluetooth device can
#   be found or paired, and paired ones can stop working;
# - Bluetooth Audio Gateway Service (BTAGService): the hands-free profile of
#   headsets (their microphone, calls).
# Read-only. Result codes: missing (either is not there: no Bluetooth
# support on this Windows) / disabled (bthserv is) / audio-disabled (only
# BTAGService is) / ok. Both are fixed by hardware.enable-bluetooth-services.
# Facts: bthserv, btag (their Start values, '' when missing).

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

$support = Get-StartValue 'bthserv'
$gateway = Get-StartValue 'BTAGService'
$facts = [ordered]@{ bthserv = ''; btag = '' }
if ($null -ne $support) {
    $facts.bthserv = $support
}
if ($null -ne $gateway) {
    $facts.btag = $gateway
}

$result = 'ok'
if (($null -eq $support) -or ($null -eq $gateway)) {
    $result = 'missing'
}
elseif ($support -eq 4) {
    $result = 'disabled'
}
elseif ($gateway -eq 4) {
    $result = 'audio-disabled'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
