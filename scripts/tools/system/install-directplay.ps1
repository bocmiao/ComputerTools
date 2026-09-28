# Tool: system.install-directplay (action)
# Turns on DirectPlay, the old DirectX networking part that games from before
# 2008 or so need ("An app on your PC needs the following Windows feature:
# DirectPlay"): the Windows feature DirectPlay under "Legacy Components", as
# ticking it in "Turn Windows features on or off" does (Dism /online
# /enable-feature /featurename:DirectPlay /All). Its files come with Windows:
# nothing is downloaded, as a rule.
# Result codes: already / installed / restart / download-failed (Windows
# wanted to download its files and could not) / corrupt / missing / failed /
# slow (Dism.exe still running after 4 minutes: it is left to finish) /
# unsupported (Windows Server). Facts: code (Dism.exe's exit code, 0x...).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block optional-feature: identical in tools/system/install-netfx3.ps1 and tools/system/install-directplay.ps1 (medkit-data check compares them) ----
# Windows optional features ("Turn Windows features on or off") are turned on
# with Dism.exe /Online /Enable-Feature /FeatureName:<name> /All (with the
# features it belongs to) /NoRestart /Quiet, started hidden through the shell
# (the host talks to medkit over its own output, which Dism.exe must not
# inherit). Its exit code is 0, 3010 (done, a restart finishes it) or an
# error code, an HRESULT such as 0x800F0906 (Microsoft, ".NET Framework 3.5
# installation errors").

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32", and
# the 32-bit Dism.exe cannot service 64-bit Windows.
$featureSystemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $featureSystemDir = Join-Path $env:windir 'Sysnative'
}

# Windows Server has its own way (Server Manager); ProductType 1 is a
# workstation.
function Test-Workstation {
    try {
        return ([int](Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop).ProductType -eq 1)
    }
    catch {
        Write-Verbose ('could not read the product type: ' + $_.Exception.Message)
        return $true
    }
}

# The state of a feature: enabled, pending (turned on, a restart finishes
# it), disabled, missing (Windows has no feature by that name) or unknown.
function Get-FeatureState {
    param([string]$Name)
    try {
        $feature = Get-WindowsOptionalFeature -Online -FeatureName $Name -ErrorAction Stop
    }
    catch {
        # 0x800F080C: no feature by that name.
        if ($_.Exception.HResult -eq -2146498548) {
            return 'missing'
        }
        Write-Verbose ('could not read the feature: ' + $_.Exception.Message)
        return 'unknown'
    }
    if ($null -eq $feature) {
        return 'missing'
    }
    switch ([string]@($feature)[0].State) {
        'Enabled' {
            return 'enabled'
        }
        'EnablePending' {
            return 'pending'
        }
        'Disabled' {
            return 'disabled'
        }
        'DisabledWithPayloadRemoved' {
            return 'disabled'
        }
        'DisablePending' {
            return 'disabled'
        }
    }
    return 'unknown'
}

# Turns a feature on: Dism.exe's exit code, or $null when it is still running
# at the deadline (it is left to finish).
function Enable-Feature {
    param([string]$Name, [DateTime]$Deadline)
    $arguments = '/Online /Enable-Feature /FeatureName:' + $Name + ' /All /NoRestart /Quiet'
    $start = New-Object Diagnostics.ProcessStartInfo((Join-Path $featureSystemDir 'Dism.exe'), $arguments)
    $start.UseShellExecute = $true
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.WorkingDirectory = $featureSystemDir
    $process = [Diagnostics.Process]::Start($start)
    $wait = [int][Math]::Max(1000, ($Deadline - [DateTime]::UtcNow).TotalMilliseconds)
    if (-not $process.WaitForExit($wait)) {
        return $null
    }
    return [int]$process.ExitCode
}

# An exit code as the result code it leads to: installed, restart, download
# (the files could not be downloaded: 0x800F0906 CBS_E_DOWNLOAD_FAILURE,
# 0x800F081F CBS_E_SOURCE_MISSING, 0x800F0950, 0x800F0954, and the Windows
# Update and network errors 0x8024xxxx, 0x80072xxx), policy (0x800F0907
# CBS_E_GROUPPOLICY_DISALLOWED: a policy says never to download them),
# service-disabled (0x80070422: a service it needs is disabled), corrupt
# (0x800F0831 CBS_E_STORE_CORRUPTION, 0x80073712 ERROR_SXS_COMPONENT_STORE_CORRUPT:
# the component store is damaged), missing (0x800F080C: no such feature) or
# failed.
function Get-EnableResult {
    param([int]$Code)
    if ($Code -eq 0) {
        return 'installed'
    }
    if ($Code -eq 3010) {
        return 'restart'
    }
    $hex = '0x{0:X8}' -f $Code
    if (@('0x800F0906', '0x800F081F', '0x800F0950', '0x800F0954') -contains $hex) {
        return 'download'
    }
    if ($hex.StartsWith('0x8024') -or $hex.StartsWith('0x80072')) {
        return 'download'
    }
    switch ($hex) {
        '0x800F0907' {
            return 'policy'
        }
        '0x80070422' {
            return 'service-disabled'
        }
        '0x800F0831' {
            return 'corrupt'
        }
        '0x80073712' {
            return 'corrupt'
        }
        '0x800F080C' {
            return 'missing'
        }
    }
    return 'failed'
}
# ---- end of shared block optional-feature ----

# Dism.exe gets 4 minutes: the tool times out after 5.
$deadline = [DateTime]::UtcNow.AddMinutes(4)

$facts = [ordered]@{ code = '' }

if (-not (Test-Workstation)) {
    [pscustomobject]@{ result = 'unsupported'; facts = $facts }
    return
}

$state = Get-FeatureState 'DirectPlay'
if ($state -eq 'enabled') {
    [pscustomobject]@{ result = 'already'; facts = $facts }
    return
}
if ($state -eq 'pending') {
    [pscustomobject]@{ result = 'restart'; facts = $facts }
    return
}
if ($state -eq 'missing') {
    [pscustomobject]@{ result = 'missing'; facts = $facts }
    return
}

$code = Enable-Feature 'DirectPlay' $deadline
if ($null -eq $code) {
    [pscustomobject]@{ result = 'slow'; facts = $facts }
    return
}
$facts.code = '0x{0:X8}' -f $code
$result = Get-EnableResult $code
if (@('download', 'policy', 'service-disabled') -contains $result) {
    $result = 'download-failed'
}
elseif (($result -eq 'installed') -and ((Get-FeatureState 'DirectPlay') -eq 'pending')) {
    $result = 'restart'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
