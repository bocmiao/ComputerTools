# Check: hardware.memory-size
# Installed memory and memory slots.
#   total:      sum of Win32_PhysicalMemory.Capacity (installed modules); falls back
#               to Win32_ComputerSystem.TotalPhysicalMemory (usable memory) when
#               the modules are not reported
#   slots:      Win32_PhysicalMemoryArray.MemoryDevices of the system-memory
#               arrays (Use = 3)
#   used slots: number of Win32_PhysicalMemory modules
# Soldered memory and some firmware report slot counts that do not match the
# board; the YAML text keeps that in mind.
# Read-only. Result codes: ok (more than 4 GB) / low (4 GB or less).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$modules = @(Get-CimInstance -ClassName Win32_PhysicalMemory | Where-Object { ($null -ne $_.Capacity) -and ([double]$_.Capacity -gt 0) })

$installedBytes = [double]0
foreach ($m in $modules) {
    $installedBytes += [double]$m.Capacity
}
if ($installedBytes -le 0) {
    $computer = Get-CimInstance -ClassName Win32_ComputerSystem
    $installedBytes = [double]$computer.TotalPhysicalMemory
}
if ($installedBytes -le 0) {
    throw 'Cannot determine the amount of installed memory'
}

$slotsTotal = 0
foreach ($array in @(Get-CimInstance -ClassName Win32_PhysicalMemoryArray)) {
    # Use: 3 = system memory (others are video memory, flash, cache, ...).
    if (($null -ne $array.Use) -and ([int]$array.Use -ne 3)) {
        continue
    }
    if ($null -ne $array.MemoryDevices) {
        $slotsTotal += [int]$array.MemoryDevices
    }
}
$slotsUsed = $modules.Count
# Firmware that reports fewer slots than modules is wrong; assume all slots are used.
if ($slotsTotal -lt $slotsUsed) {
    $slotsTotal = $slotsUsed
}
$freeSlots = $slotsTotal - $slotsUsed

$result = 'ok'
if ($installedBytes -le 4GB) {
    $result = 'low'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        total_gb    = [math]::Round($installedBytes / 1GB, 1)
        slots_total = $slotsTotal
        slots_used  = $slotsUsed
        free_slots  = $freeSlots
    }
}
