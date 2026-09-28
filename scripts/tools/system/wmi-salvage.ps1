# Tool: system.wmi-salvage (action)
# Microsoft's first repair for an inconsistent WMI repository ("winmgmt";
# "WMI: Repository Corruption, or Not?"): "winmgmt /salvagerepository"
# rebuilds the repository and merges in whatever of the old one can still be
# read; then "winmgmt /verifyrepository" again to see whether it is consistent
# now. The repository is checked first, and nothing is done when it is
# consistent.
# Not done here: "winmgmt /resetrepository" (back to the state right after
# Windows was installed: what programs registered in WMI is lost) and deleting
# the repository folder, which Microsoft warns can damage Windows and installed
# programs. They are left to a helper.
# The exit codes: 0 = worked (consistent), 1358 = ERROR_INTERNAL_DB_CORRUPTION
# (not consistent); the text output is localized and not read.
# Needs administrator rights (medkit runs elevated).
# Results: consistent (nothing to do) / done (consistent after the salvage) /
# still-inconsistent / failed (WinMgmt.exe returned another code).
# Facts: step (the command: winmgmt /verifyrepository or
# winmgmt /salvagerepository), exit_code (decimal, or 0x........).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$inconsistent = 1358

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$winmgmt = Join-Path $systemDir 'wbem\WinMgmt.exe'

# An exit code as people search for it: small ones in decimal, HRESULTs in hex.
function Format-ExitCode {
    param([int64]$Code)
    if (($Code -ge 0) -and ($Code -le 65535)) {
        return [string]$Code
    }
    if ($Code -lt 0) {
        $Code += 4294967296
    }
    return ('0x{0:X8}' -f $Code)
}

# Runs WinMgmt.exe (absolute path; its output is localized and not read).
# Returns the exit code, or -1 when it could not be started.
function Invoke-Winmgmt {
    param([string]$Switch)
    $ErrorActionPreference = 'Continue'
    try {
        $null = & $winmgmt $Switch 2>&1
        return [int64]$LASTEXITCODE
    }
    catch {
        Write-Verbose ('Could not start WinMgmt.exe: {0}' -f $_.Exception.Message)
        return [int64]-1
    }
}

function New-Result {
    param([string]$Code, [string]$Step, [int64]$ExitCode)
    return [pscustomobject]@{
        result = $Code
        facts  = [ordered]@{
            step      = $Step
            exit_code = (Format-ExitCode $ExitCode)
        }
    }
}

$code = Invoke-Winmgmt -Switch '/verifyrepository'
if ($code -eq 0) {
    New-Result -Code 'consistent' -Step 'winmgmt /verifyrepository' -ExitCode $code
    return
}
if ($code -ne $inconsistent) {
    New-Result -Code 'failed' -Step 'winmgmt /verifyrepository' -ExitCode $code
    return
}

$code = Invoke-Winmgmt -Switch '/salvagerepository'
if (($code -ne 0) -and ($code -ne $inconsistent)) {
    New-Result -Code 'failed' -Step 'winmgmt /salvagerepository' -ExitCode $code
    return
}

$code = Invoke-Winmgmt -Switch '/verifyrepository'
if ($code -eq 0) {
    New-Result -Code 'done' -Step 'winmgmt /verifyrepository' -ExitCode $code
}
elseif ($code -eq $inconsistent) {
    New-Result -Code 'still-inconsistent' -Step 'winmgmt /verifyrepository' -ExitCode $code
}
else {
    New-Result -Code 'failed' -Step 'winmgmt /verifyrepository' -ExitCode $code
}
