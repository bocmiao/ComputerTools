# Feature: power.longer-timeouts -- run (and prepare)
# Plugged in, the display turns off after 15 minutes and the PC sleeps after
# 30: each of the two is raised to that when it is shorter; never (0) and
# longer times stay. On battery nothing changes. powercfg changes the active
# plan and makes it active again, so it works at once.
# -Prepare: returns before = { plan, display_ac, display_dc, sleep_ac,
#   sleep_dc } (seconds).
# Run: -Before is that JSON. Skipped when the plan or a value changed since,
#   or when nothing is shorter; otherwise sets the values and reads them back
#   (throws when they are not what was set; the engine then runs the undo).

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = ''
)

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

$videoGroup = '7516b95f-f776-4464-8c53-06167f40cc99'
$sleepGroup = '238c9fa8-0aad-41ed-83f4-97be242c8f20'
$display = '3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e'
$sleep = '29f6c1db-86da-48c5-9fdb-f2b67b1f44da'
# Plugged in: the display turns off after 15 minutes, the PC sleeps after 30.
$displayTarget = 900
$sleepTarget = 1800

$plan = Get-ActivePlan
$now = [ordered]@{
    plan       = $plan
    display_ac = (Get-PowerSetting $plan 'AC' $display)
    display_dc = (Get-PowerSetting $plan 'DC' $display)
    sleep_ac   = (Get-PowerSetting $plan 'AC' $sleep)
    sleep_dc   = (Get-PowerSetting $plan 'DC' $sleep)
}
if ($Prepare) {
    return [pscustomobject]@{ before = $now }
}

$recorded = ConvertFrom-Json -InputObject $Before
foreach ($name in @('plan', 'display_ac', 'display_dc', 'sleep_ac', 'sleep_dc')) {
    if (($null -eq $now[$name]) -or ([string]$recorded.$name -ne [string]$now[$name])) {
        return [pscustomobject]@{ skipped = $true }
    }
}

$displayAc = [int]$now.display_ac
$sleepAc = [int]$now.sleep_ac
$raiseDisplay = ($displayAc -gt 0) -and ($displayAc -lt $displayTarget)
$raiseSleep = ($sleepAc -gt 0) -and ($sleepAc -lt $sleepTarget)
if ((-not $raiseDisplay) -and (-not $raiseSleep)) {
    return [pscustomobject]@{ skipped = $true }
}
if ($raiseDisplay) {
    $displayAc = $displayTarget
    if (-not (Set-PowerSetting $plan $videoGroup $display $displayAc ([int]$now.display_dc))) {
        throw 'The display timeout could not be changed'
    }
}
if ($raiseSleep) {
    $sleepAc = $sleepTarget
    if (-not (Set-PowerSetting $plan $sleepGroup $sleep $sleepAc ([int]$now.sleep_dc))) {
        throw 'The sleep timeout could not be changed'
    }
}
if (((Get-PowerSetting $plan 'AC' $display) -ne $displayAc) -or ((Get-PowerSetting $plan 'AC' $sleep) -ne $sleepAc)) {
    throw 'The timeouts did not change'
}
[pscustomobject]@{ after = [ordered]@{ plan = $plan; display_ac = $displayAc; sleep_ac = $sleepAc } }
