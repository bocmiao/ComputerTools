# Feature: disk.reduce-hiberfile -- break (tests only)
# Switches the hibernation file to the full one: powercfg /hibernate /type full.
# PCs without a hibernation file (virtual machines, usually), with a battery or
# with a size set by hand never get here: the disk.hiberfile check reports them
# as na, so the engine does not offer the feature there and tests skip it.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

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

$exitCode = Invoke-Powercfg @('/hibernate', '/type', 'full')
if ($exitCode -ne 0) {
    throw ('powercfg /hibernate /type full failed (exit code {0})' -f $exitCode)
}
[pscustomobject]@{ result = 'ok' }
