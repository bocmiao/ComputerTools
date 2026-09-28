# Runs the check scripts/checks/display/gpus.ps1 against made-up machines, for
# tests/gpus_script.rs. The CI machines are virtual (Hyper-V video only), so
# the real runs there only see "no graphics hardware"; here a function named
# Get-CimInstance stands in for WMI (a function wins over the cmdlet of the
# same name) and every case says which result it must give.
# Output: one line per case ("ok" or "BAD"), then "bad=<number of BAD lines>".

[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Script)

$ErrorActionPreference = 'Stop'

$displayGuid = '{4d36e968-e325-11ce-bfc1-08002be10318}'
$otherGuid = '{4d36e97e-e325-11ce-bfc1-08002be10318}'

function New-Device {
    param([string]$Id, [string]$Name, [int]$Code = 0, [string]$Service = 'drv', [string]$Class = 'Display',
        [string]$Guid = $displayGuid, [string[]]$Compatible = @())
    [pscustomobject]@{
        DeviceID               = $Id
        Name                   = $Name
        ConfigManagerErrorCode = $Code
        Service                = $Service
        PNPClass               = $Class
        ClassGuid              = $Guid
        CompatibleID           = $Compatible
        Present                = $true
    }
}

function New-Controller {
    param([string]$Id, $Width)
    [pscustomobject]@{ PNPDeviceID = $Id; CurrentHorizontalResolution = $Width }
}

# The machine the stand-in WMI describes. Global: inside the check script
# "script:" would mean the check's own scope.
$global:MedkitTestMachine = $null

function Get-CimInstance {
    param([string]$ClassName, [string]$Filter)
    $m = $global:MedkitTestMachine
    switch ($ClassName) {
        'Win32_PnPEntity' { return $m.Devices }
        'Win32_VideoController' { return $m.Controllers }
        'Win32_ComputerSystem' { return [pscustomobject]@{ PCSystemType = $m.SystemType } }
        'Win32_Battery' {
            if ($m.Battery) {
                return [pscustomobject]@{ Name = 'battery' }
            }
            return
        }
        'Win32_SystemEnclosure' { return [pscustomobject]@{ ChassisTypes = $m.Chassis } }
    }
    throw "unexpected WMI class $ClassName"
}

$bad = 0
function Test-Case {
    param([string]$Name, [string]$Want, $Machine, [hashtable]$Facts = @{})
    $global:MedkitTestMachine = $Machine
    $r = & $Script
    $ok = ($r.result -eq $Want)
    foreach ($key in $Facts.Keys) {
        $got = ($r.facts[$key] | ConvertTo-Json -Compress)
        if ($got -ne ($Facts[$key] | ConvertTo-Json -Compress)) {
            $ok = $false
        }
    }
    $mark = 'ok '
    if (-not $ok) {
        $mark = 'BAD'
        $script:bad++
    }
    '{0} {1,-44} {2,-17} {3}' -f $mark, $Name, $r.result, ($r.facts | ConvertTo-Json -Compress)
}

function New-Machine {
    param([object[]]$Devices, [object[]]$Controllers = @(), [int]$SystemType = 2, [bool]$Battery = $true,
        [int[]]$Chassis = @(10))
    [pscustomobject]@{
        Devices     = $Devices
        Controllers = $Controllers
        SystemType  = $SystemType
        Battery     = $Battery
        Chassis     = $Chassis
    }
}

# ---- one adapter on a laptop: integrated, discrete or other by vendor and name
$names = @(
    @('10DE', 'NVIDIA GeForce RTX 3060 Laptop GPU', 'discrete'),
    @('10DE', 'NVIDIA GeForce MX450', 'discrete'),
    @('8086', 'Intel(R) UHD Graphics 620', 'integrated'),
    @('8086', 'Intel(R) Iris(R) Xe Graphics', 'integrated'),
    @('8086', 'Intel(R) Iris(R) Xe MAX Graphics', 'discrete'),
    @('8086', 'Intel(R) Arc(TM) Graphics', 'integrated'),
    @('8086', 'Intel(R) Arc(TM) 140V GPU (16GB)', 'integrated'),
    @('8086', 'Intel(R) Arc(TM) A770 Graphics', 'discrete'),
    @('8086', 'Intel(R) Arc(TM) A370M Graphics', 'discrete'),
    @('8086', 'Intel(R) Arc(TM) B580 Graphics', 'discrete'),
    @('8086', 'Intel(R) Arc(TM) Pro A60M Graphics', 'discrete'),
    @('8086', 'Intel(R) HD Graphics 4600', 'integrated'),
    @('1002', 'AMD Radeon(TM) Graphics', 'integrated'),
    @('1002', 'AMD Radeon(TM) Vega 8 Graphics', 'integrated'),
    @('1002', 'Radeon(TM) RX Vega 10 Graphics', 'integrated'),
    @('1002', 'AMD Radeon(TM) 780M', 'integrated'),
    @('1002', 'AMD Radeon 8060S Graphics', 'integrated'),
    @('1002', 'AMD Radeon R7 Graphics', 'integrated'),
    @('1002', 'AMD Radeon HD 8610G', 'integrated'),
    @('1002', 'AMD Radeon RX 6600M', 'discrete'),
    @('1002', 'AMD Radeon(TM) RX 7600S', 'discrete'),
    @('1002', 'Radeon RX 580 Series', 'discrete'),
    @('1002', 'Radeon RX Vega 64', 'discrete'),
    @('1002', 'AMD Radeon (TM) Pro WX 3200 Graphics', 'discrete'),
    @('1002', 'AMD FirePro W4100', 'discrete'),
    @('1002', 'AMD Radeon R7 M260', 'discrete'),
    @('1002', 'AMD Radeon HD 7700 Series', 'discrete'),
    @('1002', 'AMD Radeon RX 9070 XT', 'discrete'),
    @('1002', 'AMD Radeon Mystery', 'other'),
    @('1ED5', 'MTT S80', 'discrete'),
    @('QCOM', 'Qualcomm(R) Adreno(TM) X1-85 GPU', 'integrated')
)
foreach ($n in $names) {
    $id = 'PCI\VEN_{0}&DEV_0001&SUBSYS_00000000&REV_00\4&1&0&0008' -f $n[0]
    if ($n[0] -eq 'QCOM') {
        $id = 'ACPI\QCOM0C11\2&DABA3FF&0'
    }
    Test-Case -Name $n[1] -Want $n[2] -Machine (New-Machine -Devices @(New-Device $id $n[1]) -Controllers @(New-Controller $id 1920))
}

# ---- whole machines
$intel = 'PCI\VEN_8086&DEV_9A49&SUBSYS_00000000&REV_01\3&11583659&0&10'
$nv = 'PCI\VEN_10DE&DEV_2520&SUBSYS_00000000&REV_A1\4&1&0&0008'
$nv2 = 'PCI\VEN_10DE&DEV_1C8D&SUBSYS_00000000&REV_A1\4&2&0&0010'
$vm = 'PCI\VEN_15AD&DEV_0405&SUBSYS_040515AD&REV_00\3&4&0&78'
$usb = 'PCI\VEN_8086&DEV_A36D&SUBSYS_00000000&REV_10\3&5&0&A0'

Test-Case 'gaming laptop' 'hybrid' (New-Machine `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics'), (New-Device $nv 'NVIDIA GeForce RTX 3060 Laptop GPU')) `
        -Controllers @((New-Controller $intel 1920), (New-Controller $nv $null))) `
    @{ integrated = @('Intel(R) UHD Graphics'); discrete = @('NVIDIA GeForce RTX 3060 Laptop GPU'); desktop = $false }
Test-Case 'desktop, monitor on the motherboard' 'on-integrated' (New-Machine -SystemType 1 -Battery $false -Chassis @(3) `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics 770'), (New-Device $nv 'NVIDIA GeForce RTX 4060')) `
        -Controllers @((New-Controller $intel 2560), (New-Controller $nv 0))) `
    @{ desktop = $true; display_on = 'integrated' }
Test-Case 'desktop, monitor on the graphics card' 'discrete' (New-Machine -SystemType 1 -Battery $false -Chassis @(7) `
        -Devices @(New-Device $nv 'NVIDIA GeForce RTX 4060') -Controllers @(New-Controller $nv 2560))
Test-Case 'desktop with a UPS (counts as a battery)' 'hybrid' (New-Machine -SystemType 1 -Battery $true -Chassis @(3) `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics 770'), (New-Device $nv 'NVIDIA GeForce RTX 4060')) `
        -Controllers @(New-Controller $intel 2560))
Test-Case 'all-in-one wired like a laptop' 'hybrid' (New-Machine -SystemType 1 -Battery $false -Chassis @(13) `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics'), (New-Device $nv 'NVIDIA GeForce MX550')) `
        -Controllers @(New-Controller $intel 1920))
Test-Case 'graphics card disabled' 'discrete-problem' (New-Machine `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics'), (New-Device $nv 'NVIDIA GeForce MX450' 22)) `
        -Controllers @(New-Controller $intel 1920)) `
    @{ problem = 'disabled'; code = 22; discrete = @('NVIDIA GeForce MX450') }
Test-Case 'graphics card on the basic driver' 'discrete-problem' (New-Machine `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics'), (New-Device $nv 'Microsoft Basic Display Adapter' 0 'BasicDisplay'))) `
    @{ problem = 'no-driver'; discrete = @('NVIDIA') }
Test-Case 'graphics card without a driver (code 28)' 'discrete-problem' (New-Machine `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics'),
        (New-Device $nv '3D Video Controller' 28 '' '' $otherGuid @('PCI\VEN_10DE&CC_030200', 'PCI\CC_0302')),
        (New-Device $usb 'USB 3.0 eXtensible Host Controller' 0 'USBXHCI' 'USB' '{36fc9e60-c465-11cf-8056-444553540000}' @('PCI\CC_0C0330')))) `
    @{ problem = 'no-driver'; code = 28; discrete = @('NVIDIA'); count = 2 }
Test-Case 'graphics card with code 43' 'discrete-problem' (New-Machine `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics'), (New-Device $nv 'NVIDIA GeForce GTX 1650' 43))) `
    @{ problem = 'error'; code = 43 }
Test-Case 'graphics card waits for a restart' 'discrete-problem' (New-Machine `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics'), (New-Device $nv 'NVIDIA GeForce GTX 1650' 14))) `
    @{ problem = 'restart' }
Test-Case 'one of two graphics cards disabled' 'hybrid' (New-Machine -SystemType 1 -Battery $false -Chassis @(3) `
        -Devices @((New-Device $intel 'Intel(R) UHD Graphics 770'), (New-Device $nv 'NVIDIA GeForce RTX 4060'), (New-Device $nv2 'NVIDIA GeForce GTX 1050' 22)) `
        -Controllers @(New-Controller $nv 2560)) `
    @{ discrete = @('NVIDIA GeForce RTX 4060'); display_on = 'discrete' }
Test-Case 'two identical graphics cards' 'discrete' (New-Machine -SystemType 1 -Battery $false -Chassis @(7) `
        -Devices @((New-Device $nv 'NVIDIA GeForce RTX 3090'), (New-Device $nv2 'NVIDIA GeForce RTX 3090')) `
        -Controllers @(New-Controller $nv 3840)) `
    @{ discrete = @('NVIDIA GeForce RTX 3090'); count = 2 }
Test-Case 'unplugged graphics box (code 45)' 'integrated' (New-Machine `
        -Devices @((New-Device $intel 'Intel(R) Iris(R) Xe Graphics'), (New-Device $nv 'NVIDIA GeForce RTX 3080' 45)) `
        -Controllers @(New-Controller $intel 1920))
Test-Case 'virtual machine' 'none' (New-Machine -SystemType 1 -Battery $false -Chassis @(1) `
        -Devices @(New-Device $vm 'VMware SVGA 3D') -Controllers @(New-Controller $vm 1024)) `
    @{ count = 0 }
Test-Case 'nothing at all' 'none' (New-Machine -Devices @())

"bad=$bad"
