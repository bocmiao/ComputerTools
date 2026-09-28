# Check: system.perf-counters
# Are Windows' performance counters switched off? Task Manager's Performance
# tab, Resource Monitor and Performance Monitor read them; "optimizer" tools
# sometimes turn them off, and then the graphs stay empty or at 0%.
# Two places switch them off (the value is "Disable Performance Counters", with
# spaces; Microsoft's newer troubleshooting article spells it
# DisablePerformanceCounters, so both spellings are read):
#   - HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Perflib: default 0,
#     1 switches off the (V1) counters of the whole system (Microsoft, "Using
#     the Registry Functions to Consume Counter Data");
#   - HKLM\SYSTEM\CurrentControlSet\Services\<service>\Performance: should be
#     0; 1 switches off that service's counters, 2 the 32-bit ones, 4 the
#     64-bit ones (Microsoft, "Manually rebuild performance counters"). Every
#     service with a Performance key is looked at.
# Perflib itself sets a service's switch to 1 when that service's counter DLL
# fails badly (Microsoft, "Disable Performance Counters Entry"), so a few
# switched-off counters of other programs and services are normal (CI 154's
# Windows Server had ASP.NET_2.0.50727, Dnscache and NetBT switched off).
# Only the whole-system switch and the system's own basic counters (PerfOS:
# processor, memory, system; PerfProc: processes; PerfDisk: disks; PerfNet and
# Tcpip: network) are a reason for empty graphs.
# The rebuild (lodctr /R) needs %SystemRoot%\System32\PerfStringBackup.INI.
# Read-only. Result codes, in this order:
#   disabled    the whole-system switch or basic counters are switched off
#               (the rebuild tool switches them on again)
#   no-backup   PerfStringBackup.INI is gone, so lodctr /R cannot rebuild
#   others      only other programs' and services' counters are switched off
#   ok
# Facts: disabled (Perflib for the whole-system switch, and basic services'
# names), disabled_count, others (the other services' names), others_count;
# names are joined with a Chinese enumeration comma.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$valueNames = @('Disable Performance Counters', 'DisablePerformanceCounters')
$basicServices = @('PerfOS', 'PerfProc', 'PerfDisk', 'PerfNet', 'Tcpip')

# Whether one of the two spellings is set to something other than 0 under a key
# (a key that is not there: no; most services have no Performance key).
function Test-CountersDisabled {
    param([string]$Path)
    $key = Get-Item -LiteralPath $Path -ErrorAction SilentlyContinue
    if ($null -eq $key) {
        return $false
    }
    foreach ($name in $valueNames) {
        $value = $key.GetValue($name)
        if (($null -ne $value) -and ($value -is [int]) -and ($value -ne 0)) {
            return $true
        }
    }
    return $false
}

$basic = New-Object System.Collections.Generic.List[string]
$others = New-Object System.Collections.Generic.List[string]
if (Test-CountersDisabled -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Perflib') {
    $basic.Add('Perflib')
}
foreach ($service in @(Get-ChildItem -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services' -ErrorAction SilentlyContinue)) {
    if (Test-CountersDisabled -Path (Join-Path $service.PSPath 'Performance')) {
        if ($basicServices -contains $service.PSChildName) {
            $basic.Add($service.PSChildName)
        }
        else {
            $others.Add($service.PSChildName)
        }
    }
}

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$backup = Join-Path $systemDir 'PerfStringBackup.INI'
$result = 'ok'
if ($basic.Count -gt 0) {
    $result = 'disabled'
}
elseif (-not (Test-Path -LiteralPath $backup -PathType Leaf)) {
    $result = 'no-backup'
}
elseif ($others.Count -gt 0) {
    $result = 'others'
}

$comma = [string][char]0x3001
[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        disabled       = ($basic.ToArray() -join $comma)
        disabled_count = $basic.Count
        others         = ($others.ToArray() -join $comma)
        others_count   = $others.Count
    }
}
