# Check: network.proxy-dead
# Does a system proxy point at a program on this PC that is no longer running?
# That is what VPN tools and game accelerators leave behind when they are
# uninstalled or crash: the proxy is on, it points at 127.0.0.1:7890, and
# nothing listens on that port, so web pages stop loading.
#
# WinINet keeps proxy settings per connection (read via -UserHive):
#   1. LAN: Internet Settings\Connections\DefaultConnectionSettings (REG_BINARY),
#      plus the legacy values Internet Settings\ProxyEnable / ProxyServer, which
#      WinINet keeps in sync and which many tools write directly.
#   2. Every dial-up / VPN (RAS) connection, for example a PPPoE "broadband
#      connection": Internet Settings\Connections\<connection name>, same binary
#      layout. WinINet uses the settings of an active dial-up / VPN connection
#      instead of the LAN settings, and some proxy tools (v2rayN, for example)
#      write the proxy into every connection.
#   SavedLegacySettings and WinHttpSettings under Connections are not
#   connections and are ignored.
# Binary layout (little-endian DWORDs; reverse-engineered by the community):
#   offset 0 version, 4 change counter, 8 flags (0x02 = manual proxy server,
#   0x04 = automatic configuration script), 12 length N of the proxy server
#   string + N bytes of ANSI text, then the bypass list and the automatic
#   configuration (PAC) URL in the same length + text form.
# LAN: the blob decides when it has the proxy on with a server string, otherwise
# the legacy values. Every dial-up / VPN connection is tested on its own.
# ProxyServer / the blob string is either "host:port" or per protocol,
# "http=host:port;https=host:port;socks=host:port", sometimes with a scheme
# ("http://127.0.0.1:7890"). For local endpoints (127.0.0.0/8, localhost, ::1,
# 0.0.0.0) a TCP connection is attempted with a 500 ms timeout; proxies on other
# machines are not tested.
# Read-only. Result codes:
#   none         no manual proxy anywhere
#   none-pac     no manual proxy, but an automatic configuration script (PAC) is set
#   remote       only proxies on other machines (not tested)
#   alive        a local proxy answers (and no local proxy is dead)
#   dead         the LAN proxy points at this PC and nothing answers
#   dead-dialup  the LAN proxy is fine or off, but a dial-up / VPN connection's
#                proxy points at this PC and nothing answers

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$connectTimeoutMs = 500
$proxyFlag = 2
$pacFlag = 4
$lanValue = 'DefaultConnectionSettings'
# Binary values under Connections that are not per-connection proxy settings.
$ignoredValues = @('SavedLegacySettings', 'WinHttpSettings')

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

# Connection blobs: the LAN one and one per dial-up / VPN connection.
$blobs = Get-ConnectionBlobs $connectionsPath

$blobPresent = $false
$blobEnabled = $false
$blobServer = ''
if ($blobs.Contains($lanValue)) {
    $lan = Read-ConnectionBlob $blobs[$lanValue]
    $blobPresent = $true
    $blobEnabled = (($lan.Flags -band $proxyFlag) -ne 0)
    $blobServer = $lan.Server
    if (($pac.Length -eq 0) -and (($lan.Flags -band $pacFlag) -ne 0)) {
        $pac = $lan.Pac
    }
}

$dialupCount = 0
$dialupOn = New-Object System.Collections.Generic.List[string]
$dialupSources = New-Object System.Collections.Generic.List[object]
foreach ($name in @($blobs.Keys)) {
    if ($name -eq $lanValue) {
        continue
    }
    $dialupCount++
    $info = Read-ConnectionBlob $blobs[$name]
    if (($info.Flags -band $proxyFlag) -ne 0) {
        $dialupOn.Add($name)
        if ($info.Server.Length -gt 0) {
            $dialupSources.Add([pscustomobject]@{ Name = $name; Server = $info.Server })
        }
    }
}

# Which LAN setting decides: the blob when it has the proxy on with a server
# string, otherwise the legacy values.
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
$facts['dialup_count'] = $dialupCount
if ($dialupOn.Count -gt 0) {
    $facts['dialup_proxy'] = ($dialupOn -join ', ')
}

# Test the LAN proxy and every dial-up / VPN proxy.
$lanState = $null
if ($source.Length -gt 0) {
    $lanState = Test-ProxyServer $server
    if ($lanState.State -eq 'unparsed') {
        throw ("Cannot parse the proxy server value '{0}'" -f $server)
    }
}
$dialupStates = New-Object System.Collections.Generic.List[object]
foreach ($s in $dialupSources) {
    $state = Test-ProxyServer $s.Server
    if ($state.State -ne 'unparsed') {
        $dialupStates.Add([pscustomobject]@{ Name = $s.Name; State = $state.State; Address = $state.Address })
    }
}

$deadDialup = @($dialupStates | Where-Object { $_.State -eq 'dead' })
$aliveDialup = @($dialupStates | Where-Object { $_.State -eq 'alive' })
$remoteDialup = @($dialupStates | Where-Object { $_.State -eq 'remote' })

if (($null -ne $lanState) -and ($lanState.State -eq 'dead')) {
    $result = 'dead'
    $facts['proxy_address'] = $lanState.Address
    if ($deadDialup.Count -gt 0) {
        $facts['dead_dialup'] = (@($deadDialup | ForEach-Object { $_.Name }) -join ', ')
    }
}
elseif ($deadDialup.Count -gt 0) {
    $result = 'dead-dialup'
    $facts['connection'] = $deadDialup[0].Name
    $facts['proxy_address'] = $deadDialup[0].Address
    $facts['dead_dialup'] = (@($deadDialup | ForEach-Object { $_.Name }) -join ', ')
}
elseif (($null -ne $lanState) -and ($lanState.State -eq 'alive')) {
    $result = 'alive'
    $facts['proxy_address'] = $lanState.Address
}
elseif ($aliveDialup.Count -gt 0) {
    $result = 'alive'
    $facts['proxy_address'] = $aliveDialup[0].Address
}
elseif (($null -ne $lanState) -and ($lanState.State -eq 'remote')) {
    $result = 'remote'
    $facts['proxy_address'] = $lanState.Address
}
elseif ($remoteDialup.Count -gt 0) {
    $result = 'remote'
    $facts['proxy_address'] = $remoteDialup[0].Address
}
elseif ($pac.Length -gt 0) {
    $result = 'none-pac'
}
else {
    $result = 'none'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
