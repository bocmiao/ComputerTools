# Check: hardware.virtualization
# Is CPU virtualization (Intel VT-x / AMD-V, "VT") turned on? Android
# emulators, WSL 2, Windows Sandbox and virtual machines need it; it is
# switched on and off in the BIOS / UEFI setup. Win32_Processor tells:
#   VirtualizationFirmwareEnabled  on in the firmware
#   VMMonitorModeExtensions        the processor supports it
# While a hypervisor runs (Hyper-V, WSL 2, Virtual Machine Platform, memory
# integrity / VBS), Windows itself sits on top of it and both read False;
# Win32_ComputerSystem.HypervisorPresent is True then, which also means
# virtualization is on.
# Read-only.
# Result codes: hypervisor (ok: on, and used by Windows) / enabled (ok) /
# disabled (advice: supported, but off in the BIOS) / unsupported (manual:
# the processor cannot do it).
# Facts: cpu (the processor model), hypervisor, firmware_enabled, supported.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Test-True {
    param($Value)
    if ($null -eq $Value) {
        return $false
    }
    if ($Value -is [bool]) {
        return $Value
    }
    return ([string]$Value).Trim() -eq 'True'
}

$system = Get-CimInstance -ClassName Win32_ComputerSystem -Property HypervisorPresent -ErrorAction Stop
$processors = @(Get-CimInstance -ClassName Win32_Processor -Property Name, VirtualizationFirmwareEnabled, VMMonitorModeExtensions -ErrorAction Stop)
if ($processors.Count -eq 0) {
    throw 'Win32_Processor returned no processor'
}
$cpu = $processors[0]

$hypervisor = Test-True $system.HypervisorPresent
$enabled = Test-True $cpu.VirtualizationFirmwareEnabled
$supported = Test-True $cpu.VMMonitorModeExtensions

$result = 'unsupported'
if ($hypervisor) {
    $result = 'hypervisor'
}
elseif ($enabled) {
    $result = 'enabled'
}
elseif ($supported) {
    $result = 'disabled'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        cpu              = ([string]$cpu.Name).Trim()
        hypervisor       = $hypervisor
        firmware_enabled = $enabled
        supported        = $supported
    }
}
