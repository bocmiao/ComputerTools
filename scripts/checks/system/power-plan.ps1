# Check: system.power-plan
# Is the power plan holding the processor back? (See the shared block for
# what is read.) Read-only. Result codes (in this order):
#   saver       the Power saver plan is active (fix: power.balanced-plan)
#   throttled   "Maximum processor state" of the active plan is below 80
#               percent plugged in, or below 50 percent on battery on a PC
#               that has a battery (fix: power.processor-full-speed)
#   efficiency  Windows 11 power mode "Best power efficiency" while plugged
#               in: ActiveOverlayAcPowerScheme
#               961cc777-2547-4f9d-8174-7d86181b8a7a (Microsoft, "Customize
#               the Windows performance power slider"); changed in Settings
#   ok
# Facts: plan (saver, balanced, high, ultimate or custom), ac, dc ("Maximum
# processor state" in percent, '' when unknown), battery (true / false).

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

$maxProcessorState = 'bc5038f7-23e0-4960-96da-33abaf5935ec'
$powerSaverPlan = 'a1841308-3541-4fab-bc81-f71556f20b4a'
$balancedPlan = '381b4222-f694-41f0-9685-ff5bb260df2e'
$bestEfficiencyMode = '961cc777-2547-4f9d-8174-7d86181b8a7a'

$planNames = @{
    'a1841308-3541-4fab-bc81-f71556f20b4a' = 'saver'
    '381b4222-f694-41f0-9685-ff5bb260df2e' = 'balanced'
    '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c' = 'high'
    'e9a42b02-d5df-448d-aa00-03f14749eb61' = 'ultimate'
}

$plan = Get-ActivePlan
$ac = Get-PowerSetting $plan 'AC' $maxProcessorState
$dc = Get-PowerSetting $plan 'DC' $maxProcessorState
$battery = @(Get-CimInstance -ClassName 'Win32_Battery' -ErrorAction SilentlyContinue).Count -gt 0
$overlay = ''
try {
    $overlay = ([string](Get-ItemProperty -LiteralPath $powerSchemesKey -Name 'ActiveOverlayAcPowerScheme' -ErrorAction Stop).ActiveOverlayAcPowerScheme).Trim().Trim('{', '}').ToLowerInvariant()
}
catch {
    Write-Verbose 'No power mode (Windows 10, or not the Balanced plan)'
}

$name = 'custom'
if ($planNames.ContainsKey($plan)) {
    $name = $planNames[$plan]
}
$facts = [ordered]@{ plan = $name; ac = ''; dc = ''; battery = $battery }
if ($null -ne $ac) {
    $facts.ac = $ac
}
if ($null -ne $dc) {
    $facts.dc = $dc
}

$result = 'ok'
if ($plan -eq $powerSaverPlan) {
    $result = 'saver'
}
elseif ((($null -ne $ac) -and ($ac -lt 80)) -or ($battery -and ($null -ne $dc) -and ($dc -lt 50))) {
    $result = 'throttled'
}
elseif (($plan -eq $balancedPlan) -and ($overlay -eq $bestEfficiencyMode)) {
    $result = 'efficiency'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
