# Feature: disk.reduce-hiberfile -- undo
# Brings the full hibernation file back, as it was before the run script:
#   hibernation still on: powercfg /hibernate /type full, then HiberFileType
#     and HiberFileSizePercent go back to their recorded values (removed again
#     when they were missing before: powercfg writes its own values there);
#   hibernation switched off since (by the user or another program): only
#     those two values go back, so the full file returns when hibernation is
#     switched on again. Hibernation is not switched on.
# A value that cannot be removed or written makes the script fail, so the
# engine does not report the undo as done.
# -Before: the JSON the run script recorded ({ kind, enabled, type, percent }).
# Also used by the engine to roll back a run that failed half-way.

[CmdletBinding()]
param(
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block hiberfile-state: identical in checks/disk/hiberfile.ps1 and features/disk/reduce-hiberfile-detect.ps1, -run.ps1 and -undo.ps1 (medkit-data check compares them) ----
# The hibernation file as powercfg manages it, in
# HKLM\SYSTEM\CurrentControlSet\Control\Power:
#   HibernateEnabled      0 = hibernation off
#   HiberFileType         1 = reduced (fast startup only), 2 = full;
#                         missing = the default Windows picked
#   HiberFileSizePercent  a size set by hand, in percent of memory;
#                         0 or missing = Windows manages the size
# and the file itself, hiberfil.sys in the root of the system drive.
# Kind, in this order: off (no hibernation file), reduced (HiberFileType 1),
# custom (a size set by hand: powercfg treats it as full and cannot reduce it
# before the size is reset), full (HiberFileType 2), else told by the size of
# the file (Windows makes a full file 40 percent of memory and a reduced one 20
# percent), or unknown.
$powerKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Power'

function Get-PowerDword {
    param($Properties, [string]$Name)
    if ($null -eq $Properties) {
        return $null
    }
    $property = $Properties.PSObject.Properties[$Name]
    if (($null -eq $property) -or ($null -eq $property.Value)) {
        return $null
    }
    try {
        return [int64]$property.Value
    }
    catch {
        return $null
    }
}

function Get-HiberState {
    $properties = $null
    if (Test-Path -LiteralPath $powerKey) {
        $properties = Get-ItemProperty -LiteralPath $powerKey
    }
    $size = [double]0
    $file = Get-Item -LiteralPath ($env:SystemDrive + '\hiberfil.sys') -Force -ErrorAction SilentlyContinue
    if (($null -ne $file) -and (-not $file.PSIsContainer)) {
        $size = [double]$file.Length
    }
    $computer = Get-CimInstance -ClassName Win32_ComputerSystem
    $memory = [double]$computer.TotalPhysicalMemory
    $state = [pscustomobject]@{
        Kind    = 'unknown'
        Enabled = Get-PowerDword $properties 'HibernateEnabled'
        Type    = Get-PowerDword $properties 'HiberFileType'
        Percent = Get-PowerDword $properties 'HiberFileSizePercent'
        Size    = $size
        Memory  = $memory
        # PCSystemType 2 = mobile (a laptop, even with the battery taken out)
        Mobile  = ([int]$computer.PCSystemType -eq 2)
    }
    if (($state.Enabled -eq 0) -or ($size -le 0)) {
        $state.Kind = 'off'
    }
    elseif ($state.Type -eq 1) {
        $state.Kind = 'reduced'
    }
    elseif (($null -ne $state.Percent) -and ($state.Percent -gt 0)) {
        $state.Kind = 'custom'
    }
    elseif ($state.Type -eq 2) {
        $state.Kind = 'full'
    }
    elseif ($memory -gt 0) {
        if (($size / $memory) -lt 0.3) {
            $state.Kind = 'reduced'
        }
        else {
            $state.Kind = 'full'
        }
    }
    return $state
}

# For a full file: why reducing is not offered. 'battery': a PC with a battery
# (laptop, tablet, a UPS that reports as one) or a laptop by its system type:
# hibernation keeps the work when the battery runs out. 'small': reducing would
# free less than 1 GB (the reduced file is 20 percent of memory). '' when the
# file can be reduced.
function Get-HiberBlock {
    param($State)
    if ($State.Mobile -or (@(Get-CimInstance -ClassName Win32_Battery).Count -gt 0)) {
        return 'battery'
    }
    if (($State.Size - ($State.Memory * 0.2)) -lt 1GB) {
        return 'small'
    }
    return ''
}
# ---- end of shared block hiberfile-state ----

# ---- shared block hiberfile-powercfg: identical in features/disk/reduce-hiberfile-run.ps1, -undo.ps1 and -break.ps1 (medkit-data check compares them) ----
# powercfg.exe of this Windows (the 64-bit one, also from a 32-bit PowerShell).
# Its output is localized and is not read: success is judged by the exit code,
# and the callers read the registry back.
$systemDir = $env:SystemRoot + '\System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = $env:SystemRoot + '\Sysnative'
}
$powercfg = $systemDir + '\powercfg.exe'

function Invoke-Powercfg {
    param([string[]]$Arguments)
    # With ErrorActionPreference 'Stop', Windows PowerShell 5.1 would turn any
    # stderr line of a native command into a terminating error.
    $ErrorActionPreference = 'Continue'
    $null = & $powercfg @Arguments 2>&1
    return $LASTEXITCODE
}
# ---- end of shared block hiberfile-powercfg ----

# Puts a DWORD under the Power key back to its recorded value: removed when it
# was missing, left alone when it already has that value.
function Restore-PowerDword {
    param([string]$Name, $Value)
    $now = $null
    if (Test-Path -LiteralPath $powerKey) {
        $now = Get-PowerDword (Get-ItemProperty -LiteralPath $powerKey) $Name
    }
    if ($null -eq $Value) {
        if ($null -ne $now) {
            Remove-ItemProperty -LiteralPath $powerKey -Name $Name -ErrorAction Stop
        }
    }
    elseif ($now -ne [int64]$Value) {
        Set-ItemProperty -LiteralPath $powerKey -Name $Name -Value ([int]$Value) -Type DWord
    }
}

$recorded = ConvertFrom-Json -InputObject $Before
if ((Get-HiberState).Kind -ne 'off') {
    $exitCode = Invoke-Powercfg @('/hibernate', '/type', 'full')
    if ($exitCode -ne 0) {
        throw ('powercfg /hibernate /type full failed (exit code {0})' -f $exitCode)
    }
}
# powercfg writes its own values; the recorded ones make the state what it was.
Restore-PowerDword -Name 'HiberFileType' -Value $recorded.type
Restore-PowerDword -Name 'HiberFileSizePercent' -Value $recorded.percent
[pscustomobject]@{ result = 'ok' }
