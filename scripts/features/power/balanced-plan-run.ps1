# Feature: power.balanced-plan -- run (and prepare)
# Makes the Balanced plan active instead of Power saver (powercfg /setactive).
# -Prepare: returns before = { plan } (the active plan).
# Run: -Before is that JSON. Returns skipped (nothing changed) when the active
#   plan is no longer the recorded one, or is not Power saver. Otherwise makes
#   Balanced active and reads it back: it must be active now, or the script
#   throws (the engine then runs the undo script).

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block power-plan: identical in checks/system/power-plan.ps1 and features/power/balanced-plan-*.ps1, processor-full-speed-*.ps1 (medkit-data check compares them) ----
# The active power plan: ActivePowerScheme under
# HKLM\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes (a GUID). The
# built-in plans: Power saver a1841308-3541-4fab-bc81-f71556f20b4a, Balanced
# 381b4222-f694-41f0-9685-ff5bb260df2e, High performance
# 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c, Ultimate performance
# e9a42b02-d5df-448d-aa00-03f14749eb61.
# "Maximum processor state" (PROCTHROTTLEMAX in the processor subgroup), in
# percent, plugged in (AC) or on battery (DC): the value Windows uses, from
# root\cimv2\power Win32_PowerSettingDataIndex.
# powercfg.exe of this Windows (the 64-bit one, also from a 32-bit
# PowerShell) changes them. Its output is localized and is not read: success
# is judged by the exit code, and the callers read the values back.
$processorGroup = '54533251-82be-4824-96c1-47b60b740d00'
$maxProcessorState = 'bc5038f7-23e0-4960-96da-33abaf5935ec'
$powerSchemesKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes'

function Get-ActivePlan {
    $value = (Get-ItemProperty -LiteralPath $powerSchemesKey -Name 'ActivePowerScheme' -ErrorAction Stop).ActivePowerScheme
    return ([string]$value).Trim().Trim('{', '}').ToLowerInvariant()
}

# $Power is AC or DC; $null when Windows does not have the setting
function Get-MaxProcessorState {
    param([string]$Plan, [string]$Power)
    $id = 'Microsoft:PowerSettingDataIndex\{' + $Plan + '}\' + $Power + '\{' + $maxProcessorState + '}'
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

# Sets "Maximum processor state" of $Plan and makes the plan active again so
# that Windows uses it now; $true when both calls worked
function Set-MaxProcessorState {
    param([string]$Plan, [int]$Ac, [int]$Dc)
    $ok = (Invoke-Powercfg @('/setacvalueindex', $Plan, $processorGroup, $maxProcessorState, [string]$Ac)) -eq 0
    $ok = ((Invoke-Powercfg @('/setdcvalueindex', $Plan, $processorGroup, $maxProcessorState, [string]$Dc)) -eq 0) -and $ok
    $ok = ((Invoke-Powercfg @('/setactive', $Plan)) -eq 0) -and $ok
    return $ok
}
# ---- end of shared block power-plan ----

$powerSaverPlan = 'a1841308-3541-4fab-bc81-f71556f20b4a'
$balancedPlan = '381b4222-f694-41f0-9685-ff5bb260df2e'

$plan = Get-ActivePlan
if ($Prepare) {
    return [pscustomobject]@{ before = [ordered]@{ plan = $plan } }
}

$recorded = ConvertFrom-Json -InputObject $Before
if (([string]$recorded.plan -ne $plan) -or ($plan -ne $powerSaverPlan)) {
    return [pscustomobject]@{ skipped = $true }
}
$code = Invoke-Powercfg @('/setactive', $balancedPlan)
if (($code -ne 0) -or ((Get-ActivePlan) -ne $balancedPlan)) {
    throw ('The Balanced plan could not be made active (powercfg exit code {0})' -f $code)
}
[pscustomobject]@{ after = [ordered]@{ plan = $balancedPlan } }
