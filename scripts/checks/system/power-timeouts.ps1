# Check: system.power-timeouts
# Why the screen goes dark or the PC falls asleep after a short while. Reads
# the active plan's "Turn off display after" (VIDEOIDLE,
# 3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e) and "Sleep after" (STANDBYIDLE,
# 29f6c1db-86da-48c5-9fdb-f2b67b1f44da), in seconds, 0 = never (Microsoft,
# "Display idle timeout", "Sleep idle timeout"; see the shared block for how
# they are read); the logged-in user's screen saver (Control Panel\Desktop:
# ScreenSaveActive, ScreenSaveTimeOut in seconds, ScreenSaverIsSecure = ask
# for the password when it ends; the same values under
# Software\Policies\Microsoft\Windows\Control Panel\Desktop win); and the
# security policy "Interactive logon: Machine inactivity limit"
# (InactivityTimeoutSecs under HKLM\...\Policies\System), which locks the PC
# after that many seconds.
# Read-only. Result codes, the first that applies:
#   missing        the power settings cannot be read (status unknown)
#   policy-lock    the machine inactivity limit is set (by a company or
#                  security software; only explained)
#   short-display  plugged in, the display turns off within 5 minutes
#   short-sleep    plugged in, the PC sleeps within 10 minutes
#   screensaver    a screen saver that asks for the password starts within 5
#                  minutes
#   ok
# short-display and short-sleep are fixed by power.longer-timeouts.
# Facts: display_ac, display_dc, sleep_ac, sleep_dc (minutes, 0 = never,
# empty when missing); saver_min (0 when there is no screen saver),
# saver_lock; policy_min (0 when not set).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
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

$display = '3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e'
$sleep = '29f6c1db-86da-48c5-9fdb-f2b67b1f44da'
$desktopKey = Join-Path $UserHive 'Control Panel\Desktop'
$desktopPolicyKey = Join-Path $UserHive 'Software\Policies\Microsoft\Windows\Control Panel\Desktop'
$systemPolicyKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'

# A registry value as text ('' when missing).
function Get-Text {
    param([string]$Key, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Key -Name $Name -ErrorAction Stop
    }
    catch {
        return ''
    }
    return ([string]$item.$Name).Trim()
}

# A screen saver value: the policy's when it is set, else the user's.
function Get-SaverValue {
    param([string]$Name)
    $policy = Get-Text $desktopPolicyKey $Name
    if ($policy.Length -gt 0) {
        return $policy
    }
    return (Get-Text $desktopKey $Name)
}

# Seconds as minutes (one decimal), '' when missing.
function Get-Minute {
    param($Seconds)
    if ($null -eq $Seconds) {
        return ''
    }
    return [math]::Round([double]$Seconds / 60, 1)
}

$plan = Get-ActivePlan
$displayAc = Get-PowerSetting $plan 'AC' $display
$displayDc = Get-PowerSetting $plan 'DC' $display
$sleepAc = Get-PowerSetting $plan 'AC' $sleep
$sleepDc = Get-PowerSetting $plan 'DC' $sleep

$saverSeconds = 0
$saverLock = $false
$saverActive = Get-SaverValue 'ScreenSaveActive'
$saverFile = Get-SaverValue 'SCRNSAVE.EXE'
$saverTimeout = 0
if (($saverActive -eq '1') -and ($saverFile.Length -gt 0) -and [int]::TryParse((Get-SaverValue 'ScreenSaveTimeOut'), [ref]$saverTimeout)) {
    $saverSeconds = $saverTimeout
    $saverLock = ((Get-SaverValue 'ScreenSaverIsSecure') -eq '1')
}

$policySeconds = 0
$policyText = Get-Text $systemPolicyKey 'InactivityTimeoutSecs'
$policyValue = 0
if ([int]::TryParse($policyText, [ref]$policyValue) -and ($policyValue -gt 0)) {
    $policySeconds = $policyValue
}

$result = 'ok'
if (($null -eq $displayAc) -and ($null -eq $sleepAc)) {
    $result = 'missing'
}
elseif ($policySeconds -gt 0) {
    $result = 'policy-lock'
}
elseif (($null -ne $displayAc) -and ($displayAc -gt 0) -and ($displayAc -lt 300)) {
    $result = 'short-display'
}
elseif (($null -ne $sleepAc) -and ($sleepAc -gt 0) -and ($sleepAc -lt 600)) {
    $result = 'short-sleep'
}
elseif (($saverSeconds -gt 0) -and ($saverSeconds -lt 300) -and $saverLock) {
    $result = 'screensaver'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        display_ac = (Get-Minute $displayAc)
        display_dc = (Get-Minute $displayDc)
        sleep_ac   = (Get-Minute $sleepAc)
        sleep_dc   = (Get-Minute $sleepDc)
        saver_min  = (Get-Minute $saverSeconds)
        saver_lock = $saverLock
        policy_min = (Get-Minute $policySeconds)
    }
}
