# Check: system.lid-action
# What closing a laptop's lid does: "Lid close action" (LIDACTION,
# 5ca83367-6e45-459f-a27b-476b1d01c936 in the power buttons and lid subgroup)
# of the active plan, plugged in (AC) and on battery (DC): 0 Do nothing,
# 1 Sleep, 2 Hibernate, 3 Shut down (Microsoft, "Lid switch close action"; see
# the shared block for how it is read). Only laptops and tablets have a lid:
# PCSystemType 2 (mobile, also with the battery taken out) or a battery.
# Read-only. Result codes:
#   no-lid   not a laptop (status na: power.lid-close-do-nothing, which
#            verifies with this check, is then not offered)
#   missing  the setting cannot be read
#   nothing  plugged in, closing the lid does nothing
#   acts     plugged in, closing the lid makes it sleep, hibernate or shut
#            down (fixed by power.lid-close-do-nothing)
# Facts: ac, dc (the values).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block power-plan: identical in checks/system/power-plan.ps1, checks/system/wake-timers.ps1, checks/system/power-timeouts.ps1, checks/system/lid-action.ps1, checks/hardware/usb-suspend.ps1, tools/system/wake-sources.ps1 and features/power/*.ps1 (medkit-data check compares them) ----
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

$lidAction = '5ca83367-6e45-459f-a27b-476b1d01c936'

$computer = Get-CimInstance -ClassName Win32_ComputerSystem
$mobile = ([int]$computer.PCSystemType -eq 2) -or (@(Get-CimInstance -ClassName Win32_Battery -ErrorAction SilentlyContinue).Count -gt 0)

$plan = Get-ActivePlan
$ac = Get-PowerSetting $plan 'AC' $lidAction
$dc = Get-PowerSetting $plan 'DC' $lidAction
$facts = [ordered]@{ ac = ''; dc = '' }
if ($null -ne $ac) {
    $facts.ac = $ac
}
if ($null -ne $dc) {
    $facts.dc = $dc
}

$result = 'acts'
if (-not $mobile) {
    $result = 'no-lid'
}
elseif ($null -eq $ac) {
    $result = 'missing'
}
elseif ($ac -eq 0) {
    $result = 'nothing'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
