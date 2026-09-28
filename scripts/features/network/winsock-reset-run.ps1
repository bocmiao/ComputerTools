# Feature: network.winsock-reset -- run (cannot be undone: undo: none)
# Runs netsh winsock reset. Microsoft ("netsh winsock"): it resets the Winsock
# catalog to a clean state, removing any custom layered service providers
# (LSPs), to fix network problems caused by corrupted Winsock settings; name
# space provider entries are not affected. Entries whose DLL is gone go too.
# Programs already running keep what they loaded; a restart completes it.
# The state before and after is read by the check network.winsock (verify).
# netsh prints a localized line; only its exit code is judged, and its text
# goes into the error message when it fails.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$netsh = Join-Path $systemDir 'netsh.exe'
if (-not (Test-Path -LiteralPath $netsh -PathType Leaf)) {
    throw 'netsh.exe was not found'
}

# Windows PowerShell 5.1 would turn a stderr line into a terminating error
# under 'Stop', so relax it and judge by the exit code.
$previousPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $output = @(& $netsh 'winsock' 'reset' 2>&1 | ForEach-Object { [string]$_ })
    $exitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $previousPreference
}
if ($exitCode -ne 0) {
    $text = (($output | Where-Object { $_.Trim().Length -gt 0 }) -join ' ').Trim()
    throw ('netsh winsock reset exited with code {0}: {1}' -f $exitCode, $text)
}

[pscustomobject]@{
    after = [pscustomobject]@{ reset = $true }
}
