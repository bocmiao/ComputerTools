# Check: display.gpus
# The graphics of this PC: which adapters are in the processor (integrated)
# and which are graphics cards (discrete), whether a graphics card works, and
# on a desktop whether the monitor is plugged into the motherboard instead of
# the graphics card. For the symptom "games are slow, the graphics card is not
# used" and the health check.
#
# Sources: Win32_PnPEntity (every display adapter, also disabled ones and ones
# with a problem code: class Display, or PCI base class 03 in the compatible
# IDs; Present is false for unplugged ones), Win32_VideoController
# (CurrentHorizontalResolution is only set for an adapter that drives a
# display right now), Win32_ComputerSystem.PCSystemType, Win32_Battery and
# Win32_SystemEnclosure.ChassisTypes (a desktop: not mobile, no battery, a
# desktop or tower case; all-in-ones and mini PCs can be wired like laptops).
# Only hardware adapters count: device instance IDs starting with PCI\ or
# ACPI\ (the graphics of ARM processors). Remote desktop, indirect display
# (IddCx: screen-sharing and virtual-monitor software) and Hyper-V adapters sit
# on other buses; adapters that virtual machines emulate are left out by their
# PCI vendor ID.
#
# Integrated or discrete, by PCI vendor ID and name (Windows gets it from the
# driver through DXGI, which a script cannot call):
#   NVIDIA (10DE), Moore Threads (1ED5)  discrete
#   Intel (8086)    discrete: Arc A / B series (A380, A770M, B580, Pro A60)
#                   and Iris Xe MAX; everything else (UHD, Iris Xe, Arc
#                   Graphics, Arc 140V) is in the processor
#   AMD (1002)      discrete: RX 460 ... RX 9070 (RX 6600M, RX 7600S), RX Vega
#                   56 / 64, Radeon Pro, FirePro, R5 / R7 / R9 with a model
#                   number (R9 290, R7 M260), HD with four digits (HD 7770);
#                   integrated: "... Graphics" (Radeon Graphics, Vega 8
#                   Graphics, RX Vega 10 Graphics, R7 Graphics), Radeon 610M
#                   ... 890M, 8060S, HD 8610G / 7660D
#   Qualcomm (ACPI\QCOM, PCI 5143)  integrated
#   anything else   other
# State of an adapter from its problem code, as in hardware.device-problems:
# 22 / 29 disabled; 1 / 28, or the basic display driver (service BasicDisplay),
# no-driver; 14, 21, 25, 26, 46, 54, 55, 56 restart; any other code error.
#
# Read-only. Result codes, the first that applies:
#   none              no hardware display adapter (virtual machine; status na)
#   discrete-problem  no graphics card works: disabled, no driver, an error or
#                     waiting for a restart
#   on-integrated     a desktop whose monitor is on the processor graphics
#                     while the (working) graphics card drives no display: the
#                     cable is in the motherboard's port
#   hybrid            processor graphics and a working graphics card
#   discrete          only graphics cards
#   integrated        only processor graphics
#   other             adapters that could not be told apart
# Facts: integrated, discrete, other, names (up to 3 names each, as arrays);
# problem (disabled, no-driver, restart, error) and code for
# discrete-problem; count, desktop and display_on (integrated, discrete, both,
# other, none) for the logs. Only this PC's own hardware is named; device
# instance IDs are only used here, never output.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$maxNames = 3
$maxNameLength = 80

$displayClassGuid = '{4d36e968-e325-11ce-bfc1-08002be10318}'
# PCI base class 03 (display controller).
$displayIdPattern = '(?i)^PCI\\(.*&)?CC_03'
# PCI vendor IDs of adapters that virtual machines emulate: VMware,
# VirtualBox, QEMU / Bochs, Red Hat QXL, virtio, Parallels, Microsoft, Xen,
# Cirrus Logic and S3 (as QEMU and Hyper-V emulate them).
$virtualVendors = @('15AD', '80EE', '1234', '1B36', '1AF4', '1AB8', '1414', '5853', '1013', '5333')
# Vendor names for an adapter without a usable name (no driver yet).
$vendorNames = @{
    '10DE' = 'NVIDIA'
    '1002' = 'AMD'
    '8086' = 'Intel'
    '1ED5' = 'Moore Threads'
    '5143' = 'Qualcomm'
    'QCOM' = 'Qualcomm'
}
$intelDiscretePattern = '(?i)\bArc\b(\s*\(TM\))?\s*(Pro\s*)?[AB]\d{2,3}[A-Z]?\b|\bXe\s*MAX\b'
$amdDiscretePattern = '(?i)\bRX\s*\d{3,4}|\bRX\s*Vega\s*(56|64)\b|\bRadeon\b(\s*\(TM\))?\s*Pro\b|\bFirePro\b|\bR[579]\s*M?\d{3}\b|\bHD\s*\d{4}M?\b'
$amdIntegratedPattern = '(?i)\bGraphics\b|\bRadeon\b(\s*\(TM\))?\s*\d{3,4}[MS]\b|\bHD\s*\d{4}[DG]\b'
# "Microsoft Basic Display Adapter"; the Chinese name contains "ji ben xian shi".
$basicDisplayNamePattern = '(?i)\bBasic Display\b|\u57fa\u672c\u663e\u793a'
# "Video Controller" and "3D Video Controller" (the graphics card of many
# laptops is a PCI 3D controller), the names of a display adapter without any
# driver ("shi pin kong zhi qi" on a Chinese system).
$genericNamePattern = '(?i)^(3D\s*)?Video Controller\b|^(3D\s*)?\u89c6\u9891\u63a7\u5236\u5668'

# Problem codes ("Device Manager error messages"); 45 (not connected) and 47
# (prepared for safe removal, an external graphics box) are not present.
$absentCodes = @(45, 47)
$disabledCodes = @(22, 29)
$missingCodes = @(1, 28)
$restartCodes = @(14, 21, 25, 26, 46, 54, 55, 56)
# SMBIOS chassis types of desktop and tower cases: desktop, low profile
# desktop, pizza box, mini tower, tower, space-saving, lunch box.
$desktopChassis = @(3, 4, 5, 6, 7, 15, 16)

function Get-CleanText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ((([string]$Value) -replace '[\x00-\x1f]', ' ') -replace '\s+', ' ').Trim()
}

function Get-DeviceName {
    param($Device)
    foreach ($candidate in @($Device.Name, $Device.Caption, $Device.Description)) {
        $text = Get-CleanText $candidate
        if ($text.Length -gt $maxNameLength) {
            # Do not cut a surrogate pair in half.
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

# PCI vendor ID (four hex digits, upper case), QCOM for Qualcomm's ACPI
# graphics, or empty.
function Get-Vendor {
    param([string]$InstanceId)
    if ($InstanceId -match '(?i)^PCI\\VEN_([0-9A-F]{4})') {
        return $Matches[1].ToUpperInvariant()
    }
    if ($InstanceId -match '(?i)^ACPI\\QCOM') {
        return 'QCOM'
    }
    return ''
}

# integrated, discrete or other.
function Get-GpuKind {
    param([string]$Vendor, [string]$Name)
    switch ($Vendor) {
        '10DE' {
            return 'discrete'
        }
        '1ED5' {
            return 'discrete'
        }
        '8086' {
            if ($Name -match $intelDiscretePattern) {
                return 'discrete'
            }
            return 'integrated'
        }
        '1002' {
            if ($Name -match $amdDiscretePattern) {
                return 'discrete'
            }
            if ($Name -match $amdIntegratedPattern) {
                return 'integrated'
            }
            return 'other'
        }
        '5143' {
            return 'integrated'
        }
        'QCOM' {
            return 'integrated'
        }
    }
    return 'other'
}

# ok, disabled, no-driver, restart or error.
function Get-GpuState {
    param($Device, [string]$Name)
    $code = [int]$Device.ConfigManagerErrorCode
    if ($disabledCodes -contains $code) {
        return 'disabled'
    }
    if ($missingCodes -contains $code) {
        return 'no-driver'
    }
    if ($restartCodes -contains $code) {
        return 'restart'
    }
    if ($code -ne 0) {
        return 'error'
    }
    if (((Get-CleanText $Device.Service) -eq 'BasicDisplay') -or ($Name -match $basicDisplayNamePattern)) {
        return 'no-driver'
    }
    return 'ok'
}

function Test-DisplayAdapter {
    param($Device)
    if ((Get-CleanText $Device.ClassGuid) -eq $displayClassGuid) {
        return $true
    }
    if ((Get-CleanText $Device.PNPClass) -eq 'Display') {
        return $true
    }
    foreach ($id in @($Device.CompatibleID)) {
        if ((Get-CleanText $id) -match $displayIdPattern) {
            return $true
        }
    }
    return $false
}

# A desktop or tower PC: not a laptop (even with the battery taken out), no
# battery, and a desktop or tower case.
function Test-Desktop {
    try {
        $computer = Get-CimInstance -ClassName Win32_ComputerSystem
        if ([int]$computer.PCSystemType -eq 2) {
            return $false
        }
        if (@(Get-CimInstance -ClassName Win32_Battery).Count -gt 0) {
            return $false
        }
        foreach ($enclosure in @(Get-CimInstance -ClassName Win32_SystemEnclosure)) {
            foreach ($type in @($enclosure.ChassisTypes)) {
                if ($desktopChassis -contains [int]$type) {
                    return $true
                }
            }
        }
    }
    catch {
        return $false
    }
    return $false
}

# Up to $maxNames different names (ignoring case), in order.
function Get-NameList {
    param([object[]]$Gpus)
    $list = New-Object System.Collections.Generic.List[string]
    foreach ($gpu in $Gpus) {
        if ($list.Count -ge $maxNames) {
            break
        }
        $known = $false
        foreach ($existing in $list) {
            if ([string]::Equals($existing, $gpu.Name, [System.StringComparison]::OrdinalIgnoreCase)) {
                $known = $true
            }
        }
        if (-not $known) {
            $list.Add($gpu.Name)
        }
    }
    # The unary comma keeps a one-item array from being unrolled.
    return , $list.ToArray()
}

# Adapters that drive a display now, by device instance ID.
$showing = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
foreach ($vc in @(Get-CimInstance -ClassName Win32_VideoController)) {
    if ($null -eq $vc) {
        continue
    }
    $id = Get-CleanText $vc.PNPDeviceID
    if (($id.Length -gt 0) -and ([long]$vc.CurrentHorizontalResolution -gt 0)) {
        [void]$showing.Add($id)
    }
}

$filter = "ClassGuid = '$displayClassGuid' OR (ConfigManagerErrorCode <> 0 AND ConfigManagerErrorCode <> 45)"
$gpus = New-Object System.Collections.Generic.List[object]
foreach ($device in @(Get-CimInstance -ClassName Win32_PnPEntity -Filter $filter)) {
    if ($null -eq $device) {
        continue
    }
    if ($device.Present -eq $false) {
        continue
    }
    if ($absentCodes -contains [int]$device.ConfigManagerErrorCode) {
        continue
    }
    $instanceId = Get-CleanText $device.DeviceID
    if (-not ($instanceId -match '(?i)^(PCI|ACPI)\\')) {
        continue
    }
    if (-not (Test-DisplayAdapter $device)) {
        continue
    }
    $vendor = Get-Vendor $instanceId
    if ($virtualVendors -contains $vendor) {
        continue
    }
    $name = Get-DeviceName $device
    $state = Get-GpuState -Device $device -Name $name
    $kind = Get-GpuKind -Vendor $vendor -Name $name
    # Without a driver the name says nothing ("Microsoft Basic Display
    # Adapter", "Video Controller"): name the maker instead.
    $generic = ($name.Length -eq 0) -or ($name -match $basicDisplayNamePattern) -or ($name -match $genericNamePattern)
    if ($generic -and $vendorNames.ContainsKey($vendor)) {
        $name = $vendorNames[$vendor]
    }
    if ($name.Length -eq 0) {
        $name = 'GPU'
    }
    $gpus.Add([pscustomobject]@{
            Name    = $name
            Kind    = $kind
            State   = $state
            Code    = [int]$device.ConfigManagerErrorCode
            Showing = $showing.Contains($instanceId)
        })
}

$all = @($gpus.ToArray())
$integrated = @($all | Where-Object { $_.Kind -eq 'integrated' })
$discrete = @($all | Where-Object { $_.Kind -eq 'discrete' })
$other = @($all | Where-Object { $_.Kind -eq 'other' })
$working = @($discrete | Where-Object { $_.State -eq 'ok' })
$broken = @($discrete | Where-Object { $_.State -ne 'ok' })
$integratedShows = @($integrated | Where-Object { $_.Showing }).Count -gt 0
$discreteShows = @($working | Where-Object { $_.Showing }).Count -gt 0
$otherShows = @($other | Where-Object { $_.Showing }).Count -gt 0

$displayOn = 'none'
if ($integratedShows -and $discreteShows) {
    $displayOn = 'both'
}
elseif ($integratedShows) {
    $displayOn = 'integrated'
}
elseif ($discreteShows) {
    $displayOn = 'discrete'
}
elseif ($otherShows) {
    $displayOn = 'other'
}

$desktop = $false
if ($all.Count -gt 0) {
    $desktop = Test-Desktop
}

$result = 'other'
if ($all.Count -eq 0) {
    $result = 'none'
}
elseif (($broken.Count -gt 0) -and ($working.Count -eq 0)) {
    $result = 'discrete-problem'
}
elseif ($desktop -and ($working.Count -gt 0) -and $integratedShows -and (-not $discreteShows)) {
    $result = 'on-integrated'
}
elseif (($integrated.Count -gt 0) -and ($working.Count -gt 0)) {
    $result = 'hybrid'
}
elseif (($working.Count -gt 0) -and ($other.Count -eq 0)) {
    $result = 'discrete'
}
elseif (($integrated.Count -gt 0) -and ($discrete.Count -eq 0) -and ($other.Count -eq 0)) {
    $result = 'integrated'
}

$facts = [ordered]@{
    count      = $all.Count
    desktop    = $desktop
    display_on = $displayOn
}
if ($integrated.Count -gt 0) {
    $facts['integrated'] = Get-NameList $integrated
}
if ($result -eq 'discrete-problem') {
    $facts['discrete'] = Get-NameList $broken
    $facts['problem'] = $broken[0].State
    $facts['code'] = $broken[0].Code
}
elseif ($working.Count -gt 0) {
    $facts['discrete'] = Get-NameList $working
}
if ($other.Count -gt 0) {
    $facts['other'] = Get-NameList $other
}
if ($all.Count -gt 0) {
    $facts['names'] = Get-NameList $all
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
