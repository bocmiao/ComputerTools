[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$service = Get-Service -Name W32Time -ErrorAction Stop
if ($service.StartType -eq 'Disabled') {
    [pscustomobject]@{ result = 'disabled'; facts = @{} }
    return
}

if ($service.Status -ne 'Running') {
    try {
        Start-Service -Name W32Time -ErrorAction Stop
    }
    catch {
        [pscustomobject]@{ result = 'service-failed'; facts = @{} }
        return
    }
}

# w32tm keeps the computer's configured time source (including an AD domain
# source). Its localized output is not parsed; the exit code reports whether
# the resynchronization request was accepted.
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$w32tm = Join-Path $systemDir 'w32tm.exe'
$previousPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $null = & $w32tm '/resync' 2>&1
    $exitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $previousPreference
}

[pscustomobject]@{
    result = if ($exitCode -eq 0) { 'requested' } else { 'no-time-data' }
    facts  = @{}
}
