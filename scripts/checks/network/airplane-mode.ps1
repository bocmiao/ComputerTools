# Check: network.airplane-mode
# Is airplane mode on? It switches every radio off: WiFi, Bluetooth, mobile
# broadband. The state is the default value of the key
# HKLM\SYSTEM\CurrentControlSet\Control\RadioManagement\SystemRadioState
# (DWORD, 1 = on, 0 = off); a value named SystemRadioState under
# RadioManagement is read as a fallback. Microsoft does not document it;
# community guides and a Microsoft forum moderator point at it. The same rule
# as network.connectivity uses.
# Read-only. Result codes: on / off / none (the key is not there: no radios,
# no airplane mode, e.g. most desktops and servers).
# Facts: airplane_mode (true / false, '' when unknown).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$radioKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\RadioManagement\SystemRadioState'
$radioParentKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\RadioManagement'

# A registry value, or $null when the key or the value does not exist or
# cannot be read. '(default)' is the default value of the key.
function Get-RegistryValue {
    param([string]$Path, [string]$Name)
    $item = $null
    try {
        $item = Get-ItemProperty -LiteralPath $Path -ErrorAction Stop
    }
    catch {
        return $null
    }
    if ($null -eq $item) {
        return $null
    }
    $prop = $item.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    return $prop.Value
}

# A whole number, or -1 when the value is missing or not a number.
function Get-Number {
    param($Value)
    $n = [long]0
    if (($null -ne $Value) -and [long]::TryParse(([string]$Value).Trim(), [ref]$n)) {
        return $n
    }
    return [long]-1
}

$value = Get-RegistryValue -Path $radioKey -Name '(default)'
if ($null -eq $value) {
    $value = Get-RegistryValue -Path $radioParentKey -Name 'SystemRadioState'
}
$n = Get-Number $value

$result = 'none'
$facts = [ordered]@{ airplane_mode = '' }
if ($n -eq 1) {
    $result = 'on'
    $facts.airplane_mode = $true
}
elseif ($n -eq 0) {
    $result = 'off'
    $facts.airplane_mode = $false
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
