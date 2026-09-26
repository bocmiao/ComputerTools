# Feature: network.proxy-off -- detect
# Is the manual system proxy switched off in both places that hold it?
#   Internet Settings\ProxyEnable (legacy value, DWORD, 1 = on)
#   Internet Settings\Connections\DefaultConnectionSettings (REG_BINARY that
#   WinINet and .NET actually read; flag 0x02 of the DWORD at offset 8 = on)
# States:
#   applied      neither place has the proxy on
#   not-applied  both have it on (or, when the blob does not exist, the legacy value has it on)
#   partial      only one of them has it on
# The engine judges this feature with "verify: network.proxy-dead"; this script is
# the fallback required for script features. Read-only.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$proxyFlag = 2

# Returns the raw value, or $null when the key or the value does not exist.
function Get-RegistryValue {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $item = Get-ItemProperty -LiteralPath $Path
    if ($null -eq $item) {
        return $null
    }
    $prop = $item.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    # The unary comma keeps a byte[] from being unrolled into single bytes.
    return , $prop.Value
}

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

$legacyOn = $false
$enable = Get-RegistryValue -Path $settingsPath -Name 'ProxyEnable'
if ($null -ne $enable) {
    $legacyOn = ([int]$enable -eq 1)
}

$blobPresent = $false
$blobOn = $false
$blob = Get-RegistryValue -Path $connectionsPath -Name 'DefaultConnectionSettings'
if (($null -ne $blob) -and ($blob -is [byte[]]) -and ($blob.Length -ge 12)) {
    $blobPresent = $true
    $blobOn = (([System.BitConverter]::ToUInt32($blob, 8) -band $proxyFlag) -ne 0)
}

if (-not $blobPresent) {
    if ($legacyOn) {
        $state = 'not-applied'
    }
    else {
        $state = 'applied'
    }
}
elseif ((-not $legacyOn) -and (-not $blobOn)) {
    $state = 'applied'
}
elseif ($legacyOn -and $blobOn) {
    $state = 'not-applied'
}
else {
    $state = 'partial'
}

$facts = [ordered]@{
    legacy_proxy_enabled = $legacyOn
}
if ($blobPresent) {
    $facts['blob_proxy_enabled'] = $blobOn
}

[pscustomobject]@{
    state = $state
    facts = $facts
}
