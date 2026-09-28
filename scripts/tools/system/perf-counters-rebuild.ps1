# Tool: system.perf-counters-rebuild (action)
# Rebuilds Windows' performance counters, for Task Manager's Performance tab,
# Resource Monitor or Performance Monitor showing empty graphs, 0% or missing
# counters. The steps are Microsoft's "Manually rebuild performance counters":
#   1. counters that are switched off are switched on again: the value
#      "Disable Performance Counters" (also spelled DisablePerformanceCounters)
#      under HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Perflib (the
#      switch of the whole system) and under every service's Performance key
#      is set back to 0, the default;
#   2. lodctr /R in System32, and in SysWOW64 on 64-bit Windows: rebuilds the
#      counter settings from the system's own PerfStringBackup.INI (without
#      it there is nothing to rebuild from, and only step 1 is done);
#   3. winmgmt /resyncperf: syncs the counters with WMI.
# Microsoft also restarts the Performance Logs and Alerts and WMI services;
# restarting the PC afterwards does that for every service, so the result
# asks for a restart instead of stopping WMI under running programs.
# lodctr's exit code 0 only means the command line was right (Microsoft,
# "lodctr"), so a run that started every step is reported as done; a step that
# could not start or returned another code (also when run a second time) is
# reported as failed.
# Needs administrator rights (medkit runs elevated).
# Results: done / switched-on (PerfStringBackup.INI is gone: switched on, not
# rebuilt) / no-backup (PerfStringBackup.INI is gone and nothing was switched
# off: nothing done) / failed.
# Facts: enabled (switches set back to 0), step (the command that failed:
# lodctr /R, SysWOW64\lodctr /R or winmgmt /resyncperf).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$valueNames = @('Disable Performance Counters', 'DisablePerformanceCounters')

function New-Result {
    param([string]$Code, [int]$Enabled, [string]$Step)
    return [pscustomobject]@{
        result = $Code
        facts  = [ordered]@{
            enabled = $Enabled
            step    = $Step
        }
    }
}

# Sets both spellings of the switch back to 0 under a key where they are set to
# something else. Returns how many were changed.
function Enable-Counters {
    param([string]$Path)
    $key = Get-Item -LiteralPath $Path -ErrorAction SilentlyContinue
    if ($null -eq $key) {
        return 0
    }
    $changed = 0
    foreach ($name in $valueNames) {
        $value = $key.GetValue($name)
        if (($null -ne $value) -and ($value -is [int]) -and ($value -ne 0)) {
            Set-ItemProperty -LiteralPath $Path -Name $name -Value 0 -Type DWord
            $changed++
        }
    }
    return $changed
}

# Runs a program from the Windows folder in its own folder (Microsoft's steps
# cd into System32 and SysWOW64 before lodctr /R) and waits. Its exit code, or
# -1 when it could not be started.
function Invoke-SystemProgram {
    param([string]$Path, [string]$Arguments)
    try {
        $process = Start-Process -FilePath $Path -ArgumentList $Arguments -WorkingDirectory (Split-Path -Parent $Path) -WindowStyle Hidden -Wait -PassThru
        return [int]$process.ExitCode
    }
    catch {
        Write-Verbose ('Could not start {0}: {1}' -f $Path, $_.Exception.Message)
        return -1
    }
}

# 1. Switch the counters on again
$enabled = Enable-Counters -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Perflib'
foreach ($service in @(Get-ChildItem -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services' -ErrorAction SilentlyContinue)) {
    $enabled += Enable-Counters -Path (Join-Path $service.PSPath 'Performance')
}

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$system32 = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $system32 = Join-Path $env:windir 'Sysnative'
}
if (-not (Test-Path -LiteralPath (Join-Path $system32 'PerfStringBackup.INI') -PathType Leaf)) {
    if ($enabled -gt 0) {
        New-Result -Code 'switched-on' -Enabled $enabled -Step ''
    }
    else {
        New-Result -Code 'no-backup' -Enabled 0 -Step ''
    }
    return
}

# 2. Rebuild, 64-bit and 32-bit
$steps = New-Object System.Collections.Generic.List[object]
$steps.Add(@('lodctr /R', (Join-Path $system32 'lodctr.exe'), '/R'))
$wow64 = Join-Path $env:windir 'SysWOW64\lodctr.exe'
if ([Environment]::Is64BitOperatingSystem -and (Test-Path -LiteralPath $wow64 -PathType Leaf)) {
    $steps.Add(@('SysWOW64\lodctr /R', $wow64, '/R'))
}
# 3. Sync with WMI
$steps.Add(@('winmgmt /resyncperf', (Join-Path $system32 'wbem\WinMgmt.exe'), '/resyncperf'))

# A step that fails is run once more: lodctr /R sometimes stops with "Unable to
# rebuild performance counter setting from system backup store, error code is
# 2" and works the second time.
foreach ($step in $steps) {
    $code = Invoke-SystemProgram -Path $step[1] -Arguments $step[2]
    if ($code -ne 0) {
        $code = Invoke-SystemProgram -Path $step[1] -Arguments $step[2]
    }
    if ($code -ne 0) {
        Write-Verbose ('{0} returned {1}' -f $step[0], $code)
        New-Result -Code 'failed' -Enabled $enabled -Step $step[0]
        return
    }
}
New-Result -Code 'done' -Enabled $enabled -Step ''
