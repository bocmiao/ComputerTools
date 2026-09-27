# Check: hardware.device-problems
# Devices that Device Manager marks with a yellow exclamation mark (a problem
# code), and graphics running on the Microsoft Basic Display Adapter.
# Source: Win32_PnPEntity. ConfigManagerErrorCode is the "Code N" Device Manager
# shows, PNPClass the device setup class (mapped from ClassGuid when empty),
# Present is false for devices that are not attached any more (phantoms). WMI is
# asked only for devices with a problem code other than 45, and for display
# adapters (a handful), so PCs with thousands of old devices stay fast.
#
# Problem codes ("Device Manager error messages"):
#   ignored   45 not connected (phantom), 47 prepared for safe removal by the
#             user, 53 reserved for the kernel debugger (set up on purpose)
#   missing   1 not configured (no driver installed), 28 drivers not installed
#   disabled  22 disabled in Windows, 29 disabled in the firmware (BIOS)
#   restart   14 needs a restart, 21 being removed, 25 / 26 still being set up,
#             46 shutting down, 54 being reset, 55 blocked while the console
#             was locked (Kernel DMA Protection), 56 class configuration pending
#   error     every other code (10 cannot start, 43 stopped after reporting
#             problems, 31, 39, 52, ...)
# A display adapter on the inbox basic driver (service BasicDisplay, or named
# "Microsoft Basic Display Adapter") is missing its driver although its code is 0.
#
# Privacy: only this PC's own hardware is named. Paired Bluetooth devices and
# their services, audio endpoints ("Headphones (Xiaoming's AirPods)"), portable
# devices (phones, cameras, USB drive volumes), print queues and devices found
# on the network (printers, TVs, NAS) often carry names people chose, so they
# are only counted (peripheral_count). They are recognised by class
# (AudioEndpoint, WPD, PrintQueue, Bluetooth apart from this PC's own adapter)
# or by enumerator (BTHENUM, BTHLE, BTHLEDEVICE, BTHHFENUM, UMB, WSDPRINT,
# WPDBUSENUMROOT, SWD\MMDEVAPI, SWD\WPDBUSENUM, SWD\PRINTENUM, SWD\DAF...).
# Their problems are not advice: a missing driver there is nearly always a
# phone or headset service Windows has no driver for ("Bluetooth Peripheral
# Device", Code 28). This PC's own Bluetooth adapter is named by its driver
# ("Intel(R) Wireless Bluetooth(R)") and is treated like any other hardware.
# Device instance IDs, hardware IDs and serial numbers are only used here,
# never output.
#
# Network cards: PCI base class 02 (compatible ID PCI\CC_02xx), USB CDC NCM /
# ECM / MBIM or RNDIS, class Net, or a name with Wi-Fi / WLAN / 802.11 /
# Ethernet ...; always on a hardware bus (PCI, USB, SD), so VPN and other
# software adapters (ROOT, SWD) are not taken for the network card.
# Display adapters: PCI base class 03 or class Display, on PCI or ACPI.
#
# Read-only. Result codes, the first that applies:
#   missing-network-driver  a network card has no driver
#   missing-display-driver  a display adapter has no driver (or the basic one)
#   device-error            devices report an error code (before other missing
#                           drivers: a failing graphics card matters more than
#                           a chipset part without its driver)
#   missing-driver          other devices have no driver
#   needs-restart           devices wait for a restart
#   disabled                only devices someone disabled (status ok)
#   peripheral              only paired or connected devices show problems (status ok)
#   ok                      nothing to report

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# At most this many different names per list; the counts cover every device.
$maxNames = 3
$maxNameLength = 80
$unnamedDevice = 'Unknown device'

$ignoredCodes = @(45, 47, 53)
$missingCodes = @(1, 28)
$disabledCodes = @(22, 29)
$restartCodes = @(14, 21, 25, 26, 46, 54, 55, 56)

$displayClassGuid = '{4d36e968-e325-11ce-bfc1-08002be10318}'
# Setup class names by class GUID, for devices whose PNPClass is empty.
$classByGuid = @{
    '{4d36e972-e325-11ce-bfc1-08002be10318}' = 'Net'
    '{4d36e968-e325-11ce-bfc1-08002be10318}' = 'Display'
    '{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}' = 'Bluetooth'
    '{c166523c-fe0c-4a94-a586-f1a80cfbbf3e}' = 'AudioEndpoint'
    '{eec5ad98-8080-425f-922a-dabf3de3f69a}' = 'WPD'
    '{1ed2bbf9-11f0-4084-b21f-ad83a8e6dcdc}' = 'PrintQueue'
    '{4d36e97e-e325-11ce-bfc1-08002be10318}' = 'Unknown'
}

# Devices whose names people often choose themselves.
$personalClasses = @('AudioEndpoint', 'WPD', 'PrintQueue')
$personalEnumerators = @('BTHENUM', 'BTHLE', 'BTHLEDEVICE', 'BTHHFENUM', 'UMB', 'WSDPRINT', 'WPDBUSENUMROOT')
$personalIdPrefixes = @('SWD\MMDEVAPI\', 'SWD\WPDBUSENUM\', 'SWD\PRINTENUM\', 'SWD\DAF')
# Bluetooth-class devices on these enumerators are this PC's own Bluetooth
# adapter or the Windows Bluetooth stack (BTH\MS_...), named by their drivers.
$bluetoothOwnEnumerators = @('USB', 'PCI', 'ACPI', 'SD', 'BTH')

$networkBuses = @('PCI', 'USB', 'SD')
$displayBuses = @('PCI', 'ACPI')
# PCI base class 02 (network controller); USB CDC NCM / ECM / MBIM and RNDIS.
$networkIdPattern = '(?i)^(PCI\\(.*&)?CC_02|USB\\CLASS_02&SUBCLASS_(06|0D|0E)|USB\\CLASS_EF&SUBCLASS_04&PROT_01|USB\\CLASS_E0&SUBCLASS_01&PROT_03)'
# PCI base class 03 (display controller).
$displayIdPattern = '(?i)^PCI\\(.*&)?CC_03'
# Network cards without a class code (USB Wi-Fi sticks name themselves); the
# Chinese words for network card, network controller and Ethernet as \u escapes.
$networkNamePattern = '(?i)\b(wi-?fi|wlan|wireless (lan|network)|ethernet|gbe|lan (adapter|card)|network (adapter|controller|card|connection))\b|802\.11|\u7f51\u5361|\u7f51\u7edc\u63a7\u5236\u5668|\u4ee5\u592a\u7f51'
# "Bluetooth", and the Chinese word for it.
$bluetoothNamePattern = '(?i)bluetooth|\u84dd\u7259'
# "Microsoft Basic Display Adapter"; the Chinese name contains "ji ben xian shi".
$basicDisplayNamePattern = '(?i)\bBasic Display\b|\u57fa\u672c\u663e\u793a'

function Get-CleanText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ((([string]$Value) -replace '[\x00-\x1f]', ' ') -replace '\s+', ' ').Trim()
}

function Get-DeviceClass {
    param($Device)
    $class = Get-CleanText $Device.PNPClass
    if ($class.Length -gt 0) {
        return $class
    }
    $guid = (Get-CleanText $Device.ClassGuid).ToLowerInvariant()
    if ($classByGuid.ContainsKey($guid)) {
        return $classByGuid[$guid]
    }
    return 'Unknown'
}

function Get-DeviceName {
    param($Device)
    foreach ($candidate in @($Device.Name, $Device.Caption, $Device.Description)) {
        $text = Get-CleanText $candidate
        if ($text.Length -gt $maxNameLength) {
            # Do not cut a surrogate pair (emoji) in half.
            $cut = $maxNameLength
            if ([char]::IsHighSurrogate($text[$cut - 1])) {
                $cut--
            }
            $text = $text.Substring(0, $cut).TrimEnd() + '...'
        }
        if ($text.Length -gt 0) {
            return $text
        }
    }
    return ''
}

# Hardware IDs and compatible IDs, for telling network cards and display
# adapters apart. Used only here, never output.
function Get-DeviceIds {
    param($Device)
    $ids = New-Object System.Collections.Generic.List[string]
    foreach ($id in @($Device.HardwareID) + @($Device.CompatibleID)) {
        $text = Get-CleanText $id
        if ($text.Length -gt 0) {
            $ids.Add($text)
        }
    }
    # The unary comma keeps an empty or one-item array from being unrolled.
    return , $ids.ToArray()
}

function Test-AnyMatch {
    param([string[]]$Values, [string]$Pattern)
    foreach ($value in $Values) {
        if ($value -match $Pattern) {
            return $true
        }
    }
    return $false
}

function Test-PersonalDevice {
    param([string]$Class, [string]$InstanceId, [string]$Enumerator)
    if ($personalClasses -contains $Class) {
        return $true
    }
    if ($personalEnumerators -contains $Enumerator) {
        return $true
    }
    foreach ($prefix in $personalIdPrefixes) {
        if ($InstanceId.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return (($Class -eq 'Bluetooth') -and (-not ($bluetoothOwnEnumerators -contains $Enumerator)))
}

function Test-BasicDisplay {
    param($Device, [string]$Class, [string]$Name)
    if ((Get-CleanText $Device.Service) -eq 'BasicDisplay') {
        return $true
    }
    return (($Class -eq 'Display') -and ($Name -match $basicDisplayNamePattern))
}

function Test-NetworkCard {
    param([string]$Class, [string]$Enumerator, [string]$Name, [string[]]$Ids)
    if (-not ($networkBuses -contains $Enumerator)) {
        return $false
    }
    if ($Name -match $bluetoothNamePattern) {
        return $false
    }
    if ($Class -eq 'Net') {
        return $true
    }
    if (Test-AnyMatch -Values $Ids -Pattern $networkIdPattern) {
        return $true
    }
    return ($Name -match $networkNamePattern)
}

function Test-DisplayAdapter {
    param([string]$Class, [string]$Enumerator, [string[]]$Ids)
    if (-not ($displayBuses -contains $Enumerator)) {
        return $false
    }
    if ($Class -eq 'Display') {
        return $true
    }
    return (Test-AnyMatch -Values $Ids -Pattern $displayIdPattern)
}

# Adds a name unless it is empty, already listed (ignoring case) or the list is full.
function Add-Name {
    param([System.Collections.Generic.List[string]]$List, [string]$Name)
    if (($Name.Length -eq 0) -or ($List.Count -ge $maxNames)) {
        return
    }
    foreach ($existing in $List) {
        if ([string]::Equals($existing, $Name, [System.StringComparison]::OrdinalIgnoreCase)) {
            return
        }
    }
    $List.Add($Name)
}

# The names of a group that has devices; never empty, so the text always has a name.
function Get-NameArray {
    param([System.Collections.Generic.List[string]]$List)
    if ($List.Count -eq 0) {
        return , @($unnamedDevice)
    }
    return , $List.ToArray()
}

$filter = "(ConfigManagerErrorCode <> 0 AND ConfigManagerErrorCode <> 45) OR Service = 'BasicDisplay' OR ClassGuid = '$displayClassGuid'"
$devices = @(Get-CimInstance -ClassName Win32_PnPEntity -Filter $filter)

$missing = 0
$missingNetwork = 0
$missingDisplay = 0
$errors = 0
$restart = 0
$disabled = 0
$peripheral = 0
$missingNames = New-Object System.Collections.Generic.List[string]
$networkNames = New-Object System.Collections.Generic.List[string]
$displayNames = New-Object System.Collections.Generic.List[string]
$errorNames = New-Object System.Collections.Generic.List[string]
$restartNames = New-Object System.Collections.Generic.List[string]
$disabledNames = New-Object System.Collections.Generic.List[string]
$errorCodes = New-Object System.Collections.Generic.List[int]
$classes = New-Object System.Collections.Generic.List[string]

foreach ($device in $devices) {
    if ($null -eq $device) {
        continue
    }
    # Unplugged devices keep an entry with Present = false (and code 45).
    if ($device.Present -eq $false) {
        continue
    }
    $code = 0
    if ($null -ne $device.ConfigManagerErrorCode) {
        $code = [int]$device.ConfigManagerErrorCode
    }
    if ($ignoredCodes -contains $code) {
        continue
    }

    $class = Get-DeviceClass $device
    $name = Get-DeviceName $device
    $instanceId = Get-CleanText $device.PNPDeviceID
    if ($instanceId.Length -eq 0) {
        $instanceId = Get-CleanText $device.DeviceID
    }
    $enumerator = (($instanceId -split '\\', 2)[0]).ToUpperInvariant()

    $basic = $false
    if ($code -eq 0) {
        # Display adapters come back even when they work; only the basic one counts.
        $basic = Test-BasicDisplay -Device $device -Class $class -Name $name
        if (-not $basic) {
            continue
        }
    }

    $knownClass = $false
    foreach ($existing in $classes) {
        if ([string]::Equals($existing, $class, [System.StringComparison]::OrdinalIgnoreCase)) {
            $knownClass = $true
            break
        }
    }
    if (-not $knownClass) {
        $classes.Add($class)
    }

    if (Test-PersonalDevice -Class $class -InstanceId $instanceId -Enumerator $enumerator) {
        $peripheral++
        continue
    }

    if ($basic -or ($missingCodes -contains $code)) {
        $missing++
        Add-Name -List $missingNames -Name $name
        $ids = Get-DeviceIds $device
        if (Test-NetworkCard -Class $class -Enumerator $enumerator -Name $name -Ids $ids) {
            $missingNetwork++
            Add-Name -List $networkNames -Name $name
        }
        elseif ($basic -or (Test-DisplayAdapter -Class $class -Enumerator $enumerator -Ids $ids)) {
            $missingDisplay++
            Add-Name -List $displayNames -Name $name
        }
    }
    elseif ($disabledCodes -contains $code) {
        $disabled++
        Add-Name -List $disabledNames -Name $name
    }
    elseif ($restartCodes -contains $code) {
        $restart++
        Add-Name -List $restartNames -Name $name
    }
    else {
        $errors++
        Add-Name -List $errorNames -Name $name
        if (-not $errorCodes.Contains($code)) {
            $errorCodes.Add($code)
        }
    }
}

$facts = [ordered]@{
    missing_count         = $missing
    missing_network_count = $missingNetwork
    missing_display_count = $missingDisplay
    error_count           = $errors
    restart_count         = $restart
    disabled_count        = $disabled
    peripheral_count      = $peripheral
}
if ($classes.Count -gt 0) {
    $facts['classes'] = (@($classes | Sort-Object) -join ', ')
}
if ($missing -gt 0) {
    $facts['missing_names'] = Get-NameArray $missingNames
}
if ($missingNetwork -gt 0) {
    $facts['missing_network_names'] = Get-NameArray $networkNames
}
if ($missingDisplay -gt 0) {
    $facts['missing_display_names'] = Get-NameArray $displayNames
}
if ($errors -gt 0) {
    $facts['error_names'] = Get-NameArray $errorNames
    $errorCodes.Sort()
    $facts['error_codes'] = $errorCodes.ToArray()
}
if ($restart -gt 0) {
    $facts['restart_names'] = Get-NameArray $restartNames
}
if ($disabled -gt 0) {
    $facts['disabled_names'] = Get-NameArray $disabledNames
}

$result = 'ok'
if ($missingNetwork -gt 0) {
    $result = 'missing-network-driver'
}
elseif ($missingDisplay -gt 0) {
    $result = 'missing-display-driver'
}
elseif ($errors -gt 0) {
    $result = 'device-error'
}
elseif ($missing -gt 0) {
    $result = 'missing-driver'
}
elseif ($restart -gt 0) {
    $result = 'needs-restart'
}
elseif ($disabled -gt 0) {
    $result = 'disabled'
}
elseif ($peripheral -gt 0) {
    $result = 'peripheral'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
