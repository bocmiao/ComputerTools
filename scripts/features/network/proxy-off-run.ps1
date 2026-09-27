# Feature: network.proxy-off -- run
# Switches off every manual proxy that points at this PC with nothing listening
# there (what network.proxy-dead reports as dead / dead-dialup):
#   LAN (DefaultConnectionSettings, or the legacy values when the blob has no
#     proxy on): Internet Settings\ProxyEnable -> DWORD 0, and flag 0x02 cleared
#     in Internet Settings\Connections\DefaultConnectionSettings
#   every dial-up / VPN connection (Internet Settings\Connections\<name>) whose
#     proxy is dead: flag 0x02 cleared
# Local proxies that answer, and proxy servers on other machines (company or
# school proxies), are left alone.
# Blobs are changed bit-wise: flag 0x02 of the DWORD at offset 8 is cleared and
# the change counter (DWORD at offset 4, wraps at 2^32) is increased by 1; every
# other byte is kept. ProxyServer, ProxyOverride, AutoConfigURL and
# SavedLegacySettings are not touched.
# The original state is recorded before anything is changed and returned as
#   before = { proxy_enable: <int or null>, legacy_changed: <bool>,
#              connections: { <value name>: <hex of the original blob> } }
# (connections lists only the blobs this script changes). The undo script gets it
# back through -Before and restores the same bits. "after" has the same shape and
# is read back from the registry after the change.
# If a write fails half-way, everything already changed is put back before the
# error is thrown.
# Running programs keep their cached proxy settings until they re-read them;
# this script cannot notify them.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:',
    [bool]$Prepare = $false,
    [string]$Before = ''
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

function ConvertTo-HexString {
    param($Bytes)
    if ($null -eq $Bytes) {
        return $null
    }
    return [System.BitConverter]::ToString([byte[]]$Bytes).Replace('-', '')
}

# A copy of the blob with flag 0x02 set or cleared and the change counter
# increased; every other byte is unchanged.
function Set-ProxyFlag {
    param([byte[]]$Blob, [bool]$On)
    $copy = [byte[]]$Blob.Clone()
    $flags = [System.BitConverter]::ToUInt32($copy, 8)
    if ($On) {
        $flags = [uint32]($flags -bor $proxyFlag)
    }
    elseif (($flags -band $proxyFlag) -ne 0) {
        $flags = [uint32]($flags - $proxyFlag)
    }
    $counter = [System.BitConverter]::ToUInt32($copy, 4)
    if ($counter -eq [uint32]::MaxValue) {
        $counter = [uint32]0
    }
    else {
        $counter = [uint32]($counter + 1)
    }
    [System.Array]::Copy([System.BitConverter]::GetBytes([uint32]$counter), 0, $copy, 4, 4)
    [System.Array]::Copy([System.BitConverter]::GetBytes([uint32]$flags), 0, $copy, 8, 4)
    # The unary comma keeps the byte[] from being unrolled into single bytes.
    return , $copy
}

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

if (-not (Test-Path -LiteralPath $settingsPath)) {
    throw ('The Internet Settings key does not exist: {0}' -f $settingsPath)
}

# 1. Record the original state and decide what to change. Nothing has been
#    changed yet, so any error here leaves the registry as it was.
$enable = Get-RegistryValue -Path $settingsPath -Name 'ProxyEnable'
$oldEnable = $null
if ($null -ne $enable) {
    $oldEnable = [int]$enable
}
$blobs = Get-ConnectionBlobs $connectionsPath
$dead = Find-DeadProxies -SettingsPath $settingsPath -Blobs $blobs

$toChange = New-Object System.Collections.Generic.List[string]
if ($dead.LanDead -and $blobs.Contains($lanValue)) {
    if (((Read-ConnectionBlob $blobs[$lanValue]).Flags -band $proxyFlag) -ne 0) {
        $toChange.Add($lanValue)
    }
}
foreach ($name in @($dead.DialupDead)) {
    $toChange.Add($name)
}
$changeLegacy = [bool]$dead.LanDead -and ($oldEnable -ne 0)

$beforeConnections = [ordered]@{}
foreach ($name in $toChange) {
    $beforeConnections[$name] = ConvertTo-HexString $blobs[$name]
}
$snapshot = [ordered]@{
    proxy_enable   = $oldEnable
    legacy_changed = $changeLegacy
    connections    = $beforeConnections
}

if ($Prepare) {
    # Engine saves this snapshot to disk before calling the script again to write.
    return [pscustomobject]@{ before = $snapshot }
}
if ([string]::IsNullOrWhiteSpace($Before)) {
    throw 'The saved pre-change state is required'
}
$savedBefore = $Before | ConvertFrom-Json
if (($null -eq $savedBefore) -or
    ($savedBefore.proxy_enable -ne $snapshot.proxy_enable) -or
    ([bool]$savedBefore.legacy_changed -ne $snapshot.legacy_changed)) {
    return [pscustomobject]@{ skipped = $true }
}
$savedConnections = @($savedBefore.connections.PSObject.Properties)
if ($savedConnections.Count -ne $beforeConnections.Count) {
    return [pscustomobject]@{ skipped = $true }
}
foreach ($entry in $savedConnections) {
    if ((-not $beforeConnections.Contains($entry.Name)) -or
        ([string]$entry.Value -ne [string]$beforeConnections[$entry.Name])) {
        return [pscustomobject]@{ skipped = $true }
    }
}

# 2. Change. If anything fails half-way, put back what was already changed
#    before reporting the error, so that a failed run leaves nothing behind.
$legacyWritten = $false
$written = New-Object System.Collections.Generic.List[string]
try {
    if ($changeLegacy) {
        Set-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable' -Value 0 -Type DWord
        $legacyWritten = $true
    }
    foreach ($name in $toChange) {
        $newBlob = Set-ProxyFlag -Blob $blobs[$name] -On $false
        Set-ItemProperty -LiteralPath $connectionsPath -Name $name -Value $newBlob -Type Binary
        $written.Add($name)
    }
}
catch {
    $failure = $_
    if ($legacyWritten) {
        if ($null -eq $oldEnable) {
            Remove-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable' -ErrorAction SilentlyContinue
        }
        else {
            Set-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable' -Value $oldEnable -Type DWord -ErrorAction SilentlyContinue
        }
    }
    foreach ($name in $written) {
        Set-ItemProperty -LiteralPath $connectionsPath -Name $name -Value ([byte[]]$blobs[$name]) -Type Binary -ErrorAction SilentlyContinue
    }
    throw $failure
}

# 3. Read back what is there now.
$newEnable = Get-RegistryValue -Path $settingsPath -Name 'ProxyEnable'
$afterEnable = $null
if ($null -ne $newEnable) {
    $afterEnable = [int]$newEnable
}
$afterBlobs = Get-ConnectionBlobs $connectionsPath
$afterConnections = [ordered]@{}
foreach ($name in $toChange) {
    $value = $null
    if ($afterBlobs.Contains($name)) {
        $value = ConvertTo-HexString $afterBlobs[$name]
    }
    $afterConnections[$name] = $value
}
$after = [ordered]@{
    proxy_enable   = $afterEnable
    legacy_changed = $changeLegacy
    connections    = $afterConnections
}

[pscustomobject]@{
    before = $snapshot
    after  = $after
    facts  = [ordered]@{
        legacy_was_enabled = ($oldEnable -eq 1)
        legacy_changed     = $changeLegacy
        lan_blob_changed   = ($toChange -contains $lanValue)
        dialup_changed     = @($dead.DialupDead).Count
    }
}
