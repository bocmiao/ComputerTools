# Check: hardware.bluetooth
# Does this PC have a Bluetooth adapter, and does it work? The adapter is a
# present Bluetooth-class device (PNPClass Bluetooth, or its class GUID) on
# the USB, PCI, ACPI or SD bus; paired phones, headsets and the Windows
# Bluetooth stack (BTH\MS_...) are not adapters. One without a driver is an
# "unknown device" (Code 28 or 1) that names itself Bluetooth, or whose
# compatible ID is the USB Bluetooth class (Class_E0&SubClass_01&Prot_01).
# Read-only. Result codes (in this order): ok (an adapter works) / disabled
# (disabled in Device Manager, Code 22) / error (another problem code) /
# no-driver / no-adapter (many desktops have no Bluetooth).
# Facts: name (the adapter's name from its driver, for ok, disabled and
# error), code (its problem code, for error).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$bluetoothClassGuid = '{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}'
$adapterBuses = @('USB', 'PCI', 'ACPI', 'SD')
# "Bluetooth", and the Chinese word for it.
$bluetoothNamePattern = '(?i)bluetooth|\u84dd\u7259'
$bluetoothClassIdPattern = '(?i)^USB\\Class_E0&SubClass_01&Prot_01'

function Get-CleanText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ((([string]$Value) -replace '[\x00-\x1f]', ' ') -replace '\s+', ' ').Trim()
}

$filter = "PNPClass = 'Bluetooth' OR ClassGuid = '$bluetoothClassGuid' OR ConfigManagerErrorCode = 28 OR ConfigManagerErrorCode = 1"
$devices = @(Get-CimInstance -ClassName Win32_PnPEntity -Filter $filter)

$working = $null
$disabled = $null
$failing = $null
$failingCode = 0
$noDriver = $false
foreach ($device in $devices) {
    if (($null -eq $device) -or ($device.Present -eq $false)) {
        continue
    }
    $code = 0
    if ($null -ne $device.ConfigManagerErrorCode) {
        $code = [int]$device.ConfigManagerErrorCode
    }
    $name = Get-CleanText $device.Name
    $id = Get-CleanText $device.PNPDeviceID
    $bus = (($id -split '\\', 2)[0]).ToUpperInvariant()
    $class = Get-CleanText $device.PNPClass
    $isBluetooth = ($class -eq 'Bluetooth') -or ((Get-CleanText $device.ClassGuid).ToLowerInvariant() -eq $bluetoothClassGuid)
    if (-not ($adapterBuses -contains $bus)) {
        continue
    }
    if (-not $isBluetooth) {
        # A device without a driver that looks like a Bluetooth adapter
        if (@(1, 28) -contains $code) {
            $ids = @($device.CompatibleID) + @($device.HardwareID) | Where-Object { $null -ne $_ }
            if (($name -match $bluetoothNamePattern) -or (@($ids | Where-Object { [string]$_ -match $bluetoothClassIdPattern }).Count -gt 0)) {
                $noDriver = $true
            }
        }
        continue
    }
    if ($code -eq 0) {
        if ($null -eq $working) {
            $working = $name
        }
    }
    elseif ($code -eq 22) {
        if ($null -eq $disabled) {
            $disabled = $name
        }
    }
    elseif ($code -ne 45) {
        if ($null -eq $failing) {
            $failing = $name
            $failingCode = $code
        }
    }
}

$facts = [ordered]@{ name = ''; code = '' }
if ($null -ne $working) {
    $result = 'ok'
    $facts.name = $working
}
elseif ($null -ne $disabled) {
    $result = 'disabled'
    $facts.name = $disabled
}
elseif ($null -ne $failing) {
    $result = 'error'
    $facts.name = $failing
    $facts.code = $failingCode
}
elseif ($noDriver) {
    $result = 'no-driver'
}
else {
    $result = 'no-adapter'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
