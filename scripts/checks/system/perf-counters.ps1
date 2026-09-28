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
# The rebuild (lodctr /R) needs %SystemRoot%\System32\PerfStringBackup.INI.
# Read-only. Result codes, in this order:
#   disabled    some counters are switched off (the rebuild tool switches
#               them on again)
#   no-backup   PerfStringBackup.INI is gone, so lodctr /R cannot rebuild
#   ok
# Facts: disabled (Perflib for the switch of the whole system, and service
# names, joined with a Chinese enumeration comma), disabled_count.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$valueNames = @('Disable Performance Counters', 'DisablePerformanceCounters')

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

$names = New-Object System.Collections.Generic.List[string]
if (Test-CountersDisabled -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Perflib') {
    $names.Add('Perflib')
}
foreach ($service in @(Get-ChildItem -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services' -ErrorAction SilentlyContinue)) {
    if (Test-CountersDisabled -Path (Join-Path $service.PSPath 'Performance')) {
        $names.Add($service.PSChildName)
    }
}

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$backup = Join-Path $systemDir 'PerfStringBackup.INI'
$result = 'ok'
if ($names.Count -gt 0) {
    $result = 'disabled'
}
elseif (-not (Test-Path -LiteralPath $backup -PathType Leaf)) {
    $result = 'no-backup'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        disabled       = ($names.ToArray() -join [string][char]0x3001)
        disabled_count = $names.Count
    }
}
