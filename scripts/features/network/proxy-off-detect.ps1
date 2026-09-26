# Feature: network.proxy-off -- detect
# Is a manual proxy that points at this PC, with nothing listening there, still
# switched on anywhere? This is exactly what network.proxy-dead reports as dead
# or dead-dialup, and exactly what proxy-off-run.ps1 switches off:
#   LAN: Internet Settings\Connections\DefaultConnectionSettings (REG_BINARY)
#        when it has the proxy on with a server string, otherwise the legacy
#        values Internet Settings\ProxyEnable / ProxyServer
#   dial-up / VPN: every other binary value under Internet Settings\Connections
#        (one per connection; SavedLegacySettings and WinHttpSettings are not
#        connections and are ignored)
# Local proxies that answer, and proxy servers on other machines, do not count.
# States:
#   applied      no dead local manual proxy is left
#   not-applied  the LAN or at least one dial-up / VPN connection still has one
# The engine judges this feature with "verify: network.proxy-dead"; this script
# is used before an undo to see whether the fix is still in place. Read-only.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$connectTimeoutMs = 500
$proxyFlag = 2
$lanValue = 'DefaultConnectionSettings'
# Binary values under Connections that are not per-connection proxy settings.
$ignoredValues = @('SavedLegacySettings', 'WinHttpSettings')

# ---- shared block proxy-test: identical in checks/network/proxy-dead.ps1, features/network/proxy-off-detect.ps1 and proxy-off-run.ps1 (medkit-data check compares them) ----
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

# ---- shared block proxy-find-dead: identical in features/network/proxy-off-detect.ps1 and proxy-off-run.ps1 (medkit-data check compares them) ----
# Which manual proxies point at this PC with nothing listening, judged the same
# way as checks/network/proxy-dead.ps1: for the LAN, the blob when it has the
# proxy on with a server string, otherwise the legacy ProxyEnable / ProxyServer;
# every dial-up / VPN connection on its own. Proxies that answer and proxies on
# other machines are never reported.
function Find-DeadProxies {
    param([string]$SettingsPath, $Blobs)
    $settings = Get-ItemProperty -LiteralPath $SettingsPath
    $legacyOn = $false
    $legacyServer = ''
    if ($null -ne $settings) {
        $enableProp = $settings.PSObject.Properties['ProxyEnable']
        if (($null -ne $enableProp) -and ($null -ne $enableProp.Value)) {
            $legacyOn = ([int]$enableProp.Value -eq 1)
        }
        $serverProp = $settings.PSObject.Properties['ProxyServer']
        if (($null -ne $serverProp) -and ($null -ne $serverProp.Value)) {
            $legacyServer = ([string]$serverProp.Value).Trim()
        }
    }
    $lanServer = ''
    if ($Blobs.Contains($lanValue)) {
        $lan = Read-ConnectionBlob $Blobs[$lanValue]
        if ((($lan.Flags -band $proxyFlag) -ne 0) -and ($lan.Server.Length -gt 0)) {
            $lanServer = $lan.Server
        }
    }
    if (($lanServer.Length -eq 0) -and $legacyOn) {
        $lanServer = $legacyServer
    }
    $lanDead = ($lanServer.Length -gt 0) -and ((Test-ProxyServer $lanServer).State -eq 'dead')

    $dialupDead = New-Object System.Collections.Generic.List[string]
    foreach ($name in @($Blobs.Keys)) {
        if ($name -eq $lanValue) {
            continue
        }
        $info = Read-ConnectionBlob $Blobs[$name]
        if ((($info.Flags -band $proxyFlag) -ne 0) -and ($info.Server.Length -gt 0)) {
            if ((Test-ProxyServer $info.Server).State -eq 'dead') {
                $dialupDead.Add($name)
            }
        }
    }
    return [pscustomobject]@{
        LanDead    = $lanDead
        DialupDead = $dialupDead.ToArray()
    }
}
# ---- end of shared block proxy-find-dead ----

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

$blobs = Get-ConnectionBlobs $connectionsPath
$dead = Find-DeadProxies -SettingsPath $settingsPath -Blobs $blobs
$dialupDead = @($dead.DialupDead)

$state = 'applied'
if ($dead.LanDead -or ($dialupDead.Count -gt 0)) {
    $state = 'not-applied'
}

$facts = [ordered]@{
    lan_dead_proxy = [bool]$dead.LanDead
}
if ($dialupDead.Count -gt 0) {
    $facts['dialup_dead_proxy'] = ($dialupDead -join ', ')
}

[pscustomobject]@{
    state = $state
    facts = $facts
}
