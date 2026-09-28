# Feature: power.processor-full-speed -- run (and prepare)
# Sets "Maximum processor state" of the active plan back to 100 percent,
# plugged in and on battery (the Windows default), and applies it now.
# -Prepare: returns before = { plan, ac, dc }.
# Run: -Before is that JSON. Returns skipped (nothing changed) when the active
#   plan or the values are no longer the recorded ones. Otherwise sets both to
#   100 and reads them back: they must be 100 now, or the script throws (the
#   engine then runs the undo script).

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

$processorGroup = '54533251-82be-4824-96c1-47b60b740d00'
$maxProcessorState = 'bc5038f7-23e0-4960-96da-33abaf5935ec'

$plan = Get-ActivePlan
$ac = Get-PowerSetting $plan 'AC' $maxProcessorState
$dc = Get-PowerSetting $plan 'DC' $maxProcessorState
if ($Prepare) {
    return [pscustomobject]@{ before = [ordered]@{ plan = $plan; ac = $ac; dc = $dc } }
}

$recorded = ConvertFrom-Json -InputObject $Before
if (($null -eq $ac) -or ($null -eq $dc) -or ([string]$recorded.plan -ne $plan) -or ([string]$recorded.ac -ne [string]$ac) -or ([string]$recorded.dc -ne [string]$dc)) {
    return [pscustomobject]@{ skipped = $true }
}
$ok = Set-PowerSetting $plan $processorGroup $maxProcessorState 100 100
if ((-not $ok) -or ((Get-PowerSetting $plan 'AC' $maxProcessorState) -ne 100) -or ((Get-PowerSetting $plan 'DC' $maxProcessorState) -ne 100)) {
    throw 'Maximum processor state could not be set to 100 percent'
}
[pscustomobject]@{ after = [ordered]@{ plan = $plan; ac = 100; dc = 100 } }
