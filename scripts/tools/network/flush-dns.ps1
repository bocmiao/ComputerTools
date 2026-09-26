# Tool: network.flush-dns
# Clears the DNS client cache: the host name -> address answers Windows
# remembered (plus the entries it preloads from the hosts file, which it reads
# again right away). No setting is changed, so this is a tool, not a feature.
#
# 1. Count the entries first with Get-DnsClientCache (fact "entries"); when that
#    is not possible the result is done-no-count.
# 2. Clear-DnsClientCache (the same as ipconfig /flushdns). When the cmdlet is
#    missing or fails (the DnsClient cmdlets go through WMI, which can be broken
#    on exactly the PCs that need this), fall back to System32\ipconfig.exe
#    /flushdns and check its exit code. The console text of ipconfig is
#    localized and is not parsed.
#
# Result codes: done / done-no-count (both ok). Facts: entries (done only),
# method (cmdlet / ipconfig).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$entries = $null
if ($null -ne (Get-Command -Name 'Get-DnsClientCache' -ErrorAction SilentlyContinue)) {
    try {
        $entries = @(Get-DnsClientCache).Count
    }
    catch {
        $entries = $null
    }
}

$method = ''
$cmdletError = 'Clear-DnsClientCache is not available'
if ($null -ne (Get-Command -Name 'Clear-DnsClientCache' -ErrorAction SilentlyContinue)) {
    try {
        $null = Clear-DnsClientCache
        $method = 'cmdlet'
    }
    catch {
        $cmdletError = $_.Exception.Message
    }
}

if ($method.Length -eq 0) {
    # 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
    $systemDir = Join-Path $env:windir 'System32'
    if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
        $systemDir = Join-Path $env:windir 'Sysnative'
    }
    $ipconfig = Join-Path $systemDir 'ipconfig.exe'
    if (-not (Test-Path -LiteralPath $ipconfig -PathType Leaf)) {
        throw ('{0}, and ipconfig.exe was not found' -f $cmdletError)
    }
    # ipconfig prints a localized result line; it is discarded. Windows
    # PowerShell 5.1 would turn a stderr line into a terminating error under
    # 'Stop', so relax it and judge by the exit code.
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $null = & $ipconfig '/flushdns' 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($exitCode -ne 0) {
        throw ('{0}, and ipconfig /flushdns exited with code {1}' -f $cmdletError, $exitCode)
    }
    $method = 'ipconfig'
}

$facts = [ordered]@{ method = $method }
$result = 'done-no-count'
if ($null -ne $entries) {
    $facts['entries'] = $entries
    $result = 'done'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
