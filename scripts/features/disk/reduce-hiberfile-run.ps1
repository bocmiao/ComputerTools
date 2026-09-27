# Feature: disk.reduce-hiberfile -- run (and prepare)
# Switches the hibernation file to the reduced one, which only fast startup
# uses (about 20 percent of memory instead of 40): powercfg /hibernate /type
# reduced. It takes effect at once, without a restart. The reduced file does
# not support hibernate or hybrid sleep.
# The engine only runs this where the disk.hiberfile check allows it (verify):
# a full file whose size Windows manages, on a PC without a battery.
# -Prepare: returns before = { kind, enabled, type, percent }: the kind of file
#   and the registry values HibernateEnabled, HiberFileType and
#   HiberFileSizePercent (null when missing). The undo script gets it back
#   through -Before.
# Run: -Before is that JSON. Returns skipped (nothing changed) when the state
#   is no longer what was recorded, the file is not the full one, or reducing
#   is not offered on this PC (a battery or a laptop, or less than 1 GB to
#   gain: Get-HiberBlock, the same rules as the check). Otherwise
#   runs powercfg and reads the state back: it must now be the reduced file
#   (HiberFileType 1, or a file of about 20 percent of memory when powercfg
#   left that value out), or the script throws (the engine then runs the undo
#   script).
# after has the same shape as before, read back after the change.

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
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

function ConvertTo-HiberSnapshot {
    param($State)
    return [ordered]@{
        kind    = $State.Kind
        enabled = $State.Enabled
        type    = $State.Type
        percent = $State.Percent
    }
}

$state = Get-HiberState
$snapshot = ConvertTo-HiberSnapshot $state
if ($Prepare) {
    return [pscustomobject]@{ before = $snapshot }
}

$recorded = ConvertFrom-Json -InputObject $Before
foreach ($name in @('kind', 'enabled', 'type', 'percent')) {
    if ([string]$recorded.$name -ne [string]$snapshot[$name]) {
        return [pscustomobject]@{ skipped = $true }
    }
}
# The engine only runs this when the disk.hiberfile check allows it; the same
# rules are checked here again, in case that check could not run.
if (($snapshot.kind -ne 'full') -or ((Get-HiberBlock $state).Length -gt 0)) {
    return [pscustomobject]@{ skipped = $true }
}

$exitCode = Invoke-Powercfg @('/hibernate', '/type', 'reduced')
if ($exitCode -ne 0) {
    throw ('powercfg /hibernate /type reduced failed (exit code {0})' -f $exitCode)
}
$after = Get-HiberState
if ($after.Kind -ne 'reduced') {
    throw 'powercfg reported success, but the hibernation file is still not the reduced one'
}
[pscustomobject]@{ after = (ConvertTo-HiberSnapshot $after) }
