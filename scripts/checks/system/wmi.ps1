# Check: system.wmi
# Is WMI (Windows Management Instrumentation) working? Installers, games,
# "System Information" (msinfo32), management tools and most of medkit's own
# checks read the computer through it; when it is broken they fail with
# "Invalid class" (0x80041010), "Not found" (0x80041002) and the like.
# Microsoft, "winmgmt" and "WMI: Repository Corruption, or Not?":
#   - the service is Winmgmt (Start = 4 under
#     HKLM\SYSTEM\CurrentControlSet\Services\Winmgmt: disabled);
#   - "winmgmt /verifyrepository" checks the repository WMI is using
#     (%windir%\System32\wbem\Repository) and returns
#     ERROR_INTERNAL_DB_CORRUPTION (1358) when it is not consistent. Its text
#     output is localized, so only the exit code is read. The first repair is
#     "winmgmt /salvagerepository" (the tool system.wmi-salvage); the
#     repository must not be deleted as a first action (that can damage
#     Windows and installed programs);
#   - when the repository is consistent, a failing query points elsewhere (a
#     provider, permissions): Win32_OperatingSystem is queried once to see
#     whether WMI answers at all (Tron's repair_wmi.bat does the same with
#     "wmic computersystem get name");
#   - an OBJECTS.DATA of 1 GB or more is too large (WMI gets slow); the only
#     way to shrink it is rebuilding the repository, which loses what programs
#     registered, so that is left to a helper.
# Read-only. Needs administrator rights (winmgmt, the Repository folder).
# Result codes, in this order:
#   disabled       the Winmgmt service is disabled (nothing else is run)
#   inconsistent   the repository is not consistent (exit code 1358)
#   broken         the query of Win32_OperatingSystem fails (facts: error)
#   too-large      OBJECTS.DATA is 1 GB or more (facts: size_mb)
#   verify-failed  winmgmt /verifyrepository returned another code
#   ok
# Facts: exit_code (winmgmt /verifyrepository: decimal, or 0x........),
# error (the query's error code, 0x........; empty when it worked),
# size_mb (OBJECTS.DATA in MB; -1 when it could not be read).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$inconsistent = 1358
$tooLargeMb = 1024

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$wbemDir = Join-Path $systemDir 'wbem'

function Format-ErrorCode {
    param($Value)
    $number = [int64]$Value
    if ($number -lt 0) {
        $number += 4294967296
    }
    return ('0x{0:X8}' -f $number)
}

# An exit code as people search for it: small ones in decimal, HRESULTs in hex.
function Format-ExitCode {
    param([int64]$Code)
    if (($Code -ge 0) -and ($Code -le 65535)) {
        return [string]$Code
    }
    return (Format-ErrorCode $Code)
}

# The error code of a failed CIM query. PowerShell puts WMI's HRESULT into the
# error's FullyQualifiedErrorId ("HRESULT 0x80041010,Microsoft.Management...");
# otherwise the innermost exception's.
function Get-QueryErrorCode {
    param($ErrorRecord)
    $match = [regex]::Match([string]$ErrorRecord.FullyQualifiedErrorId, 'HRESULT 0x([0-9A-Fa-f]{8})')
    if ($match.Success) {
        return ('0x' + $match.Groups[1].Value.ToUpperInvariant())
    }
    $inner = $ErrorRecord.Exception
    while ($null -ne $inner.InnerException) {
        $inner = $inner.InnerException
    }
    return (Format-ErrorCode $inner.HResult)
}

# Runs WinMgmt.exe (absolute path; its output is localized and not read).
# Returns the exit code, or -1 when it could not be started.
function Invoke-Winmgmt {
    param([string]$Switch)
    $ErrorActionPreference = 'Continue'
    try {
        $null = & (Join-Path $wbemDir 'WinMgmt.exe') $Switch 2>&1
        return [int64]$LASTEXITCODE
    }
    catch {
        Write-Verbose ('Could not start WinMgmt.exe: {0}' -f $_.Exception.Message)
        return [int64]-1
    }
}

$facts = [ordered]@{
    exit_code = ''
    error     = ''
    size_mb   = -1
}

$start = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\Winmgmt' -Name 'Start').Start
if ([int]$start -eq 4) {
    [pscustomobject]@{ result = 'disabled'; facts = $facts }
    return
}

$code = Invoke-Winmgmt -Switch '/verifyrepository'
$facts['exit_code'] = Format-ExitCode $code

try {
    $null = Get-CimInstance -ClassName Win32_OperatingSystem -OperationTimeoutSec 60 -ErrorAction Stop
}
catch {
    $facts['error'] = Get-QueryErrorCode $_
}

try {
    $data = Get-Item -LiteralPath (Join-Path $wbemDir 'Repository\OBJECTS.DATA') -Force -ErrorAction Stop
    $facts['size_mb'] = [int64][Math]::Round($data.Length / 1MB)
}
catch {
    Write-Verbose ('Could not read the size of OBJECTS.DATA: {0}' -f $_.Exception.Message)
}

$result = 'ok'
if ($code -eq $inconsistent) {
    $result = 'inconsistent'
}
elseif ($facts['error'].Length -gt 0) {
    $result = 'broken'
}
elseif ($facts['size_mb'] -ge $tooLargeMb) {
    $result = 'too-large'
}
elseif ($code -ne 0) {
    $result = 'verify-failed'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
