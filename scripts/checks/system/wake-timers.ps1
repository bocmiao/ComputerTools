# Check: system.wake-timers
# May wake timers wake this PC? "Allow wake timers" (RTCWAKE,
# bd3b718a-0680-4d9d-8ab2-e1d2b4ac806d in the sleep subgroup) of the active
# plan: 0 No, 1 Yes, 2 Important (internal system timers only; Microsoft,
# "Automatically wake for tasks"). With 1, scheduled tasks (Windows Update,
# other programs' timers) can wake the PC at night. (See the shared block for
# how it is read.)
# Read-only. Result codes (in this order): missing (Windows does not have the
# setting) / allowed (1 plugged in or on battery) / important (2, and not 1) /
# off (0 both). allowed and important are fixed by power.wake-timers-off.
# Facts: ac, dc (the values).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block power-plan: identical in checks/system/power-plan.ps1, checks/system/wake-timers.ps1, checks/hardware/usb-suspend.ps1, tools/system/wake-sources.ps1 and features/power/*.ps1 (medkit-data check compares them) ----
# The active power plan: ActivePowerScheme under
# HKLM\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes (a GUID). The
# built-in plans: Power saver a1841308-3541-4fab-bc81-f71556f20b4a, Balanced
# 381b4222-f694-41f0-9685-ff5bb260df2e, High performance
# 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c, Ultimate performance
# e9a42b02-d5df-448d-aa00-03f14749eb61.
# A power setting of a plan, plugged in (AC) or on battery (DC): the value
# Windows uses, from root\cimv2\power Win32_PowerSettingDataIndex (the setting
# GUID alone names it there; its subgroup does not).
# powercfg.exe of this Windows (the 64-bit one, also from a 32-bit
# PowerShell) changes them. Its output is localized and is not read: success
# is judged by the exit code, and the callers read the values back.
$powerSchemesKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes'

function Get-ActivePlan {
    $value = (Get-ItemProperty -LiteralPath $powerSchemesKey -Name 'ActivePowerScheme' -ErrorAction Stop).ActivePowerScheme
    return ([string]$value).Trim().Trim('{', '}').ToLowerInvariant()
}

# $Power is AC or DC; $null when Windows does not have the setting
function Get-PowerSetting {
    param([string]$Plan, [string]$Power, [string]$Setting)
    $id = 'Microsoft:PowerSettingDataIndex\{' + $Plan + '}\' + $Power + '\{' + $Setting + '}'
    $filter = "InstanceID = '" + $id.Replace('\', '\\') + "'"
    $item = @(Get-CimInstance -Namespace 'root\cimv2\power' -ClassName 'Win32_PowerSettingDataIndex' -Filter $filter -ErrorAction SilentlyContinue)
    if ($item.Count -eq 0) {
        return $null
    }
    return [int]$item[0].SettingIndexValue
}

$powerSystemDir = $env:SystemRoot + '\System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $powerSystemDir = $env:SystemRoot + '\Sysnative'
}

function Invoke-Powercfg {
    param([string[]]$Arguments)
    # With ErrorActionPreference 'Stop', Windows PowerShell 5.1 would turn any
    # stderr line of a native command into a terminating error.
    $ErrorActionPreference = 'Continue'
    $null = & ($powerSystemDir + '\powercfg.exe') @Arguments 2>&1
    return $LASTEXITCODE
}

# Sets a power setting of $Plan (in subgroup $Group) plugged in and on battery,
# and makes the plan active again so that Windows uses it now; $true when
# every call worked
function Set-PowerSetting {
    param([string]$Plan, [string]$Group, [string]$Setting, [int]$Ac, [int]$Dc)
    $ok = (Invoke-Powercfg @('/setacvalueindex', $Plan, $Group, $Setting, [string]$Ac)) -eq 0
    $ok = ((Invoke-Powercfg @('/setdcvalueindex', $Plan, $Group, $Setting, [string]$Dc)) -eq 0) -and $ok
    $ok = ((Invoke-Powercfg @('/setactive', $Plan)) -eq 0) -and $ok
    return $ok
}
# ---- end of shared block power-plan ----

$wakeTimers = 'bd3b718a-0680-4d9d-8ab2-e1d2b4ac806d'

$plan = Get-ActivePlan
$ac = Get-PowerSetting $plan 'AC' $wakeTimers
$dc = Get-PowerSetting $plan 'DC' $wakeTimers
$facts = [ordered]@{ ac = ''; dc = '' }
if ($null -ne $ac) {
    $facts.ac = $ac
}
if ($null -ne $dc) {
    $facts.dc = $dc
}

$result = 'off'
if (($null -eq $ac) -and ($null -eq $dc)) {
    $result = 'missing'
}
elseif (($ac -eq 1) -or ($dc -eq 1)) {
    $result = 'allowed'
}
elseif (($ac -eq 2) -or ($dc -eq 2)) {
    $result = 'important'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
