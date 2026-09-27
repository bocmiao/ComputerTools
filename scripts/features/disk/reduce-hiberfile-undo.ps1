# Feature: disk.reduce-hiberfile -- undo
# Brings the full hibernation file back, as it was before the run script:
#   hibernation still on: powercfg /hibernate /type full, then HiberFileType
#     goes back to its recorded value (removed again when it was missing
#     before: powercfg writes 2 there);
#   hibernation switched off since (by the user or another program): only
#     HiberFileType goes back to its recorded value, so the full file returns
#     when hibernation is switched on again. Hibernation is not switched on.
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
# Kind: off (no hibernation file), custom (a size set by hand: powercfg treats
# it as full and cannot reduce it before the size is reset), reduced, full or
# unknown. Without HiberFileType the kind is told by the size of the file:
# Windows makes a full file 40 percent of memory and a reduced one 20 percent.
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
    $memory = [double](Get-CimInstance -ClassName Win32_ComputerSystem).TotalPhysicalMemory
    $state = [pscustomobject]@{
        Kind    = 'unknown'
        Enabled = Get-PowerDword $properties 'HibernateEnabled'
        Type    = Get-PowerDword $properties 'HiberFileType'
        Percent = Get-PowerDword $properties 'HiberFileSizePercent'
        Size    = $size
        Memory  = $memory
    }
    if (($state.Enabled -eq 0) -or ($size -le 0)) {
        $state.Kind = 'off'
    }
    elseif (($null -ne $state.Percent) -and ($state.Percent -gt 0)) {
        $state.Kind = 'custom'
    }
    elseif ($state.Type -eq 1) {
        $state.Kind = 'reduced'
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

$recorded = ConvertFrom-Json -InputObject $Before
if ((Get-HiberState).Kind -ne 'off') {
    $exitCode = Invoke-Powercfg @('/hibernate', '/type', 'full')
    if ($exitCode -ne 0) {
        throw ('powercfg /hibernate /type full failed (exit code {0})' -f $exitCode)
    }
}
if ($null -eq $recorded.type) {
    Remove-ItemProperty -LiteralPath $powerKey -Name 'HiberFileType' -ErrorAction SilentlyContinue
}
else {
    Set-ItemProperty -LiteralPath $powerKey -Name 'HiberFileType' -Value ([int]$recorded.type) -Type DWord
}
[pscustomobject]@{ result = 'ok' }
