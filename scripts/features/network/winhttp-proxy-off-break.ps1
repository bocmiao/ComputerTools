# Feature: network.winhttp-proxy-off -- break (tests only)
# Sets the 64-bit WinHTTP proxy to 127.0.0.1:9 (the discard port, where
# nothing listens normally), the way "netsh winhttp set proxy 127.0.0.1:9"
# stores it: version 0x18, counter 0, flags 3 (direct and proxy), the server
# string, an empty bypass list.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# Local proxies get this long to accept a connection (as in network.proxy-dead).
$connectTimeoutMs = 500
# Read-ConnectionBlob and Get-ConnectionBlobs come with the shared block below;
# only the blob of WinHttpSettings is read here.
$ignoredValues = @()

# ---- shared block proxy-test: identical in checks/network/proxy-dead.ps1, checks/network/winhttp-proxy.ps1, features/network/proxy-off-detect.ps1, proxy-off-run.ps1 and winhttp-proxy-off-*.ps1 (medkit-data check compares them) ----
# Every per-connection settings blob under Connections: name -> byte[].
# Values that are not REG_BINARY or shorter than 12 bytes are skipped.
function Get-ConnectionBlobs {
    param([string]$Path)
    $result = [ordered]@{}
    if (Test-Path -LiteralPath $Path) {
        $key = Get-Item -LiteralPath $Path
        foreach ($name in @($key.GetValueNames())) {
            if ([string]::IsNullOrEmpty($name) -or ($ignoredValues -contains $name)) {
                continue
            }
            $value = $key.GetValue($name)
            if (($value -is [byte[]]) -and ($value.Length -ge 12)) {
                $result[$name] = $value
            }
        }
    }
    # The unary comma keeps the dictionary in one piece.
    return , $result
}

# Flags, proxy server string and PAC URL of one connection settings blob.
# Strings that do not fit into the blob are left empty.
function Read-ConnectionBlob {
    param([byte[]]$Blob)
    $texts = New-Object System.Collections.Generic.List[string]
    $offset = 12
    for ($i = 0; $i -lt 3; $i++) {
        if (($offset + 4) -gt $Blob.Length) {
            break
        }
        $length = [long][System.BitConverter]::ToUInt32($Blob, $offset)
        $offset += 4
        if (($offset + $length) -gt $Blob.Length) {
            break
        }
        $texts.Add(([System.Text.Encoding]::Default.GetString($Blob, $offset, [int]$length)).Trim([char]0).Trim())
        $offset += [int]$length
    }
    $server = ''
    $pacUrl = ''
    if ($texts.Count -ge 1) {
        $server = $texts[0]
    }
    if ($texts.Count -ge 3) {
        $pacUrl = $texts[2]
    }
    return [pscustomobject]@{
        Flags  = [System.BitConverter]::ToUInt32($Blob, 8)
        Server = $server
        Pac    = $pacUrl
    }
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

# Tests one proxy server string. State: remote / alive / dead, or unparsed when
# no endpoint can be read from it.
function Test-ProxyServer {
    param([string]$Server)
    $endpoints = @(Get-ProxyEndpoints $Server)
    if ($endpoints.Count -eq 0) {
        return [pscustomobject]@{ State = 'unparsed'; Address = '' }
    }
    $local = @($endpoints | Where-Object { Test-LocalHost $_.Host })
    if ($local.Count -eq 0) {
        return [pscustomobject]@{ State = 'remote'; Address = (Format-Endpoint $endpoints[0]) }
    }
    # One program usually serves every local port; if any of them answers,
    # the proxy works.
    $tested = New-Object System.Collections.Generic.List[string]
    foreach ($ep in $local) {
        $label = Format-Endpoint $ep
        if ($tested.Contains($label)) {
            continue
        }
        $tested.Add($label)
        if (Test-LocalPort -Name $ep.Host -Port $ep.Port -TimeoutMs $connectTimeoutMs) {
            return [pscustomobject]@{ State = 'alive'; Address = $label }
        }
    }
    return [pscustomobject]@{ State = 'dead'; Address = (Format-Endpoint $local[0]) }
}
# ---- end of shared block proxy-test ----

# ---- shared block winhttp-proxy: identical in checks/network/winhttp-proxy.ps1 and features/network/winhttp-proxy-off-*.ps1 (medkit-data check compares them) ----
# WinHTTP keeps its proxy (netsh winhttp set proxy / reset proxy) in the
# REG_BINARY value WinHttpSettings: under the 64-bit view for 64-bit programs
# (Windows Update, BITS, Delivery Optimization and most services), and a copy
# under WOW6432Node for 32-bit programs. The layout is that of the WinINet
# connection blobs read by Read-ConnectionBlob (offset 8 flags, 0x02 = a proxy
# server is on; offset 12 the length of the proxy server string, then the
# string); "direct access" is flags 1 with an empty string.
$winHttpViews = @(
    @('64', 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'),
    @('32', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Internet Settings\Connections')
)
$winHttpValue = 'WinHttpSettings'
$winHttpProxyFlag = 2

# The WinHttpSettings blob of one view, or $null when there is none.
function Get-WinHttpBlob {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $value = (Get-Item -LiteralPath $Path).GetValue($winHttpValue)
    if (($value -is [byte[]]) -and ($value.Length -ge 12)) {
        # The unary comma keeps the byte[] from being unrolled into single bytes.
        return , $value
    }
    return $null
}

# Every view with a proxy server on: View ('64' / '32'), Path, Blob, State
# (dead / alive / remote / unparsed, as Test-ProxyServer tells) and Port (of a
# local proxy, else '').
function Get-WinHttpProxies {
    $list = New-Object System.Collections.Generic.List[object]
    foreach ($view in $winHttpViews) {
        $blob = Get-WinHttpBlob $view[1]
        if ($null -eq $blob) {
            continue
        }
        $info = Read-ConnectionBlob $blob
        if ((($info.Flags -band $winHttpProxyFlag) -eq 0) -or ($info.Server.Length -eq 0)) {
            continue
        }
        $test = Test-ProxyServer $info.Server
        $port = ''
        if ((@('dead', 'alive') -contains $test.State) -and ($test.Address -match ':(\d+)$')) {
            $port = $Matches[1]
        }
        $list.Add([pscustomobject]@{
                View  = $view[0]
                Path  = $view[1]
                Blob  = $blob
                State = $test.State
                Port  = $port
            })
    }
    return , $list.ToArray()
}
# ---- end of shared block winhttp-proxy ----

$server = [System.Text.Encoding]::ASCII.GetBytes('127.0.0.1:9')
$bytes = New-Object System.Collections.Generic.List[byte]
foreach ($number in @(0x18, 0, 3, $server.Length)) {
    $bytes.AddRange([System.BitConverter]::GetBytes([uint32]$number))
}
$bytes.AddRange($server)
$bytes.AddRange([System.BitConverter]::GetBytes([uint32]0))
$path = $winHttpViews[0][1]
if (-not (Test-Path -LiteralPath $path)) {
    $null = New-Item -Path $path -Force
}
Set-ItemProperty -LiteralPath $path -Name $winHttpValue -Value $bytes.ToArray() -Type Binary -ErrorAction Stop

[pscustomobject]@{ result = 'ok' }
