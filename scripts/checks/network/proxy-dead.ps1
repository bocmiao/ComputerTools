# Check: network.proxy-dead
# Does the system proxy point at a program on this PC that is no longer running?
# That is what VPN tools and game accelerators leave behind when they are
# uninstalled or crash: the proxy is on, it points at 127.0.0.1:7890, and
# nothing listens on that port, so web pages stop loading.
#
# Two places hold the per-user proxy setting (read via -UserHive):
#   1. Internet Settings\Connections\DefaultConnectionSettings (REG_BINARY).
#      This is what WinINet and .NET actually use. Layout (little-endian DWORDs):
#        offset 0  version, offset 4 change counter, offset 8 flags
#        (0x02 = manual proxy server in use), offset 12 length N of the proxy
#        server string, then N bytes of ANSI text.
#   2. The legacy values Internet Settings\ProxyEnable / ProxyServer, which
#      WinINet keeps in sync and which many tools write directly.
# The blob is used when it exists and is at least 16 bytes. The proxy counts as
# on if either place says so; the endpoint that is tested comes from the blob
# when the blob has the proxy on with a server string, otherwise from the legacy
# values.
# ProxyServer / the blob string is either "host:port" or per protocol,
# "http=host:port;https=host:port;socks=host:port", sometimes with a scheme
# ("http://127.0.0.1:7890"). For local endpoints (127.0.0.0/8, localhost, ::1)
# a TCP connection is attempted with a 500 ms timeout.
# AutoConfigURL (PAC script) is reported as a fact only: the fix
# (network.proxy-off) only turns off the manual proxy.
# Read-only. Result codes: none / remote / alive / dead.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$connectTimeoutMs = 500
$proxyFlag = 2

function Get-PropertyText {
    param($Object, [string]$Name)
    if ($null -eq $Object) {
        return ''
    }
    $prop = $Object.PSObject.Properties[$Name]
    if (($null -eq $prop) -or ($null -eq $prop.Value)) {
        return ''
    }
    return ([string]$prop.Value).Trim()
}

# Returns the raw value, or $null when the key or the value does not exist.
function Get-RegistryValue {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $item = Get-ItemProperty -LiteralPath $Path
    if ($null -eq $item) {
        return $null
    }
    $prop = $item.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    # The unary comma keeps a byte[] from being unrolled into single bytes.
    return , $prop.Value
}

# Splits a proxy server string into endpoints: @{ Protocol; Host; Port }.
function Get-ProxyEndpoints {
    param([string]$Text)
    $list = New-Object System.Collections.Generic.List[object]
    foreach ($part in $Text.Split(';')) {
        $item = $part.Trim()
        if ($item.Length -eq 0) {
            continue
        }
        $protocol = ''
        $eq = $item.IndexOf('=')
        if ($eq -ge 0) {
            $protocol = $item.Substring(0, $eq).Trim().ToLowerInvariant()
            $item = $item.Substring($eq + 1).Trim()
        }
        $scheme = [regex]::Match($item, '^[A-Za-z][A-Za-z0-9+.-]*://')
        if ($scheme.Success) {
            $item = $item.Substring($scheme.Length)
        }
        $item = $item.TrimEnd('/')
        if ($item.Length -eq 0) {
            continue
        }

        $hostPart = $item
        $portText = ''
        if ($item.StartsWith('[')) {
            # [IPv6]:port
            $close = $item.IndexOf(']')
            if ($close -lt 0) {
                continue
            }
            $hostPart = $item.Substring(1, $close - 1)
            $rest = $item.Substring($close + 1)
            if ($rest.StartsWith(':')) {
                $portText = $rest.Substring(1)
            }
        }
        else {
            $colons = ($item.ToCharArray() | Where-Object { $_ -eq ':' }).Count
            if ($colons -eq 1) {
                $idx = $item.IndexOf(':')
                $hostPart = $item.Substring(0, $idx)
                $portText = $item.Substring($idx + 1)
            }
            elseif ($colons -gt 1) {
                # Bare IPv6 such as ::1 (optionally followed by :port).
                $ip = $null
                if (-not [System.Net.IPAddress]::TryParse($item, [ref]$ip)) {
                    $idx = $item.LastIndexOf(':')
                    $hostPart = $item.Substring(0, $idx)
                    $portText = $item.Substring($idx + 1)
                }
            }
        }

        $port = 0
        if ($portText.Length -eq 0) {
            # WinINet defaults: 80 for HTTP-style proxies, 1080 for SOCKS.
            if ($protocol -eq 'socks') {
                $port = 1080
            }
            else {
                $port = 80
            }
        }
        elseif ((-not [int]::TryParse($portText, [ref]$port)) -or ($port -lt 1) -or ($port -gt 65535)) {
            continue
        }
        if ($hostPart.Trim().Length -eq 0) {
            continue
        }
        $list.Add([pscustomobject]@{ Protocol = $protocol; Host = $hostPart.Trim(); Port = $port })
    }
    # Emitted one by one; callers collect them with @(...).
    return $list.ToArray()
}

function Test-SocketError {
    param($ErrorRecord)
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        if ($ex -is [System.Net.Sockets.SocketException]) {
            return $true
        }
        $ex = $ex.InnerException
    }
    return $false
}

function Test-LocalHost {
    param([string]$Name)
    $h = $Name.ToLowerInvariant()
    if (($h -eq 'localhost') -or ($h -eq '0.0.0.0')) {
        return $true
    }
    $ip = $null
    if ([System.Net.IPAddress]::TryParse($h, [ref]$ip)) {
        return [System.Net.IPAddress]::IsLoopback($ip)
    }
    return $false
}

function Format-Endpoint {
    param($Endpoint)
    if ($Endpoint.Host.Contains(':')) {
        return ('[{0}]:{1}' -f $Endpoint.Host, $Endpoint.Port)
    }
    return ('{0}:{1}' -f $Endpoint.Host, $Endpoint.Port)
}

# True if something accepts a TCP connection on the local endpoint.
function Test-LocalPort {
    param([string]$Name, [int]$Port, [int]$TimeoutMs)
    $h = $Name.ToLowerInvariant()
    $targets = @($h)
    if (($h -eq 'localhost') -or ($h -eq '0.0.0.0')) {
        $targets = @('127.0.0.1', '::1')
    }
    foreach ($t in $targets) {
        $address = [System.Net.IPAddress]::Parse($t)
        $client = $null
        try {
            $client = New-Object System.Net.Sockets.TcpClient($address.AddressFamily)
            $pending = $client.BeginConnect($address, $Port, $null, $null)
            if ($pending.AsyncWaitHandle.WaitOne($TimeoutMs, $false)) {
                # Completed: EndConnect throws when the connection was refused.
                $client.EndConnect($pending)
                return $true
            }
        }
        catch {
            # Connection refused, or IPv6 not available: nothing is listening
            # here. Any other error is a real failure and is passed on.
            if (-not (Test-SocketError $_)) {
                throw
            }
        }
        finally {
            if ($null -ne $client) {
                $client.Close()
            }
        }
    }
    return $false
}

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

# Legacy values.
$settings = Get-ItemProperty -LiteralPath $settingsPath
$enableText = Get-PropertyText $settings 'ProxyEnable'
$legacyEnabled = $false
if ($enableText.Length -gt 0) {
    $legacyEnabled = ([int]$enableText -eq 1)
}
$legacyServer = Get-PropertyText $settings 'ProxyServer'
$pac = Get-PropertyText $settings 'AutoConfigURL'

# DefaultConnectionSettings.
$blobPresent = $false
$blobEnabled = $false
$blobServer = ''
$raw = Get-RegistryValue -Path $connectionsPath -Name 'DefaultConnectionSettings'
if (($null -ne $raw) -and ($raw -is [byte[]]) -and ($raw.Length -ge 16)) {
    $blob = [byte[]]$raw
    $blobPresent = $true
    $flags = [System.BitConverter]::ToUInt32($blob, 8)
    $blobEnabled = (($flags -band $proxyFlag) -ne 0)
    $length = [System.BitConverter]::ToUInt32($blob, 12)
    if (($length -gt 0) -and ((16 + [long]$length) -le $blob.Length)) {
        $blobServer = [System.Text.Encoding]::Default.GetString($blob, 16, [int]$length).Trim([char]0).Trim()
    }
}

# Which setting decides: the blob when it has the proxy on with a server string,
# otherwise the legacy values.
$source = ''
$server = ''
if ($blobPresent -and $blobEnabled -and ($blobServer.Length -gt 0)) {
    $source = 'blob'
    $server = $blobServer
}
elseif ($legacyEnabled -and ($legacyServer.Length -gt 0)) {
    $source = 'legacy'
    $server = $legacyServer
}

$facts = [ordered]@{
    proxy_enabled        = ($source.Length -gt 0)
    proxy_server         = $server
    legacy_proxy_enabled = $legacyEnabled
}
if ($blobPresent) {
    $facts['blob_proxy_enabled'] = $blobEnabled
    $mismatch = ($legacyEnabled -ne $blobEnabled)
    if ((-not $mismatch) -and $legacyEnabled -and ($legacyServer.Length -gt 0) -and ($blobServer.Length -gt 0) -and ($legacyServer -ne $blobServer)) {
        $mismatch = $true
    }
    if ($mismatch) {
        $facts['settings_mismatch'] = $true
    }
}
if ($source.Length -gt 0) {
    $facts['proxy_source'] = $source
}
$facts['pac_url'] = $pac

if ($source.Length -eq 0) {
    $result = 'none'
}
else {
    $endpoints = @(Get-ProxyEndpoints $server)
    if ($endpoints.Count -eq 0) {
        throw ("Cannot parse the proxy server value '{0}'" -f $server)
    }
    $local = @($endpoints | Where-Object { Test-LocalHost $_.Host })
    if ($local.Count -eq 0) {
        $facts['proxy_address'] = Format-Endpoint $endpoints[0]
        $result = 'remote'
    }
    else {
        # One program usually serves every local port; if any of them answers,
        # the proxy works.
        $alive = $false
        $tested = New-Object System.Collections.Generic.List[string]
        foreach ($ep in $local) {
            $label = Format-Endpoint $ep
            if ($tested.Contains($label)) {
                continue
            }
            $tested.Add($label)
            if (Test-LocalPort -Name $ep.Host -Port $ep.Port -TimeoutMs $connectTimeoutMs) {
                $alive = $true
                $facts['proxy_address'] = $label
                break
            }
        }
        if ($alive) {
            $result = 'alive'
        }
        else {
            $facts['proxy_address'] = Format-Endpoint $local[0]
            $result = 'dead'
        }
    }
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
