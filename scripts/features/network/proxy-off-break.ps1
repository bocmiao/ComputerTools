# Feature: network.proxy-off -- break (used only by the automated round-trip test)
# Produces the "leftover proxy" state that network.proxy-dead reports as dead:
#   Internet Settings\ProxyEnable = 1, ProxyServer = 127.0.0.1:9 (discard port;
#   normally nothing listens there)
#   Connections\DefaultConnectionSettings, only if it already exists:
#     - well-formed (at least 16 bytes and the proxy string fits): rewritten as
#       version, counter + 1, flags | 0x02, new length, "127.0.0.1:9", and every
#       byte after the original proxy string copied unchanged
#     - 12 to 15 bytes or a bad string length: only flags | 0x02 and counter + 1
# A missing blob or Connections key is not created.
# The test harness restores the machine afterwards by running the fix again.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$proxyFlag = 2
$proxyServer = '127.0.0.1:9'

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

# Writes a little-endian DWORD into the array in place.
function Set-UInt32 {
    param($Bytes, [int]$Offset, [uint32]$Value)
    $b = [System.BitConverter]::GetBytes($Value)
    [System.Array]::Copy($b, 0, $Bytes, $Offset, 4)
}

function Get-NextCounter {
    param([uint32]$Value)
    if ($Value -eq [uint32]::MaxValue) {
        return [uint32]0
    }
    return [uint32]($Value + 1)
}

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

if (-not (Test-Path -LiteralPath $settingsPath)) {
    throw ('The Internet Settings key does not exist: {0}' -f $settingsPath)
}

Set-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable' -Value 1 -Type DWord
Set-ItemProperty -LiteralPath $settingsPath -Name 'ProxyServer' -Value $proxyServer -Type String

$blobMode = 'absent'
$blob = Get-RegistryValue -Path $connectionsPath -Name 'DefaultConnectionSettings'
if (($null -ne $blob) -and ($blob -is [byte[]]) -and ($blob.Length -ge 12)) {
    $flags = [System.BitConverter]::ToUInt32($blob, 8) -bor $proxyFlag
    $counter = Get-NextCounter ([System.BitConverter]::ToUInt32($blob, 4))

    $oldLength = -1
    if ($blob.Length -ge 16) {
        $oldLength = [long][System.BitConverter]::ToUInt32($blob, 12)
    }
    if (($oldLength -ge 0) -and ((16 + $oldLength) -le $blob.Length)) {
        $proxyBytes = [System.Text.Encoding]::ASCII.GetBytes($proxyServer)
        $restStart = [int](16 + $oldLength)
        $restLength = $blob.Length - $restStart
        $newBlob = New-Object byte[] (16 + $proxyBytes.Length + $restLength)
        [System.Array]::Copy($blob, 0, $newBlob, 0, 4)
        Set-UInt32 -Bytes $newBlob -Offset 4 -Value $counter
        Set-UInt32 -Bytes $newBlob -Offset 8 -Value ([uint32]$flags)
        Set-UInt32 -Bytes $newBlob -Offset 12 -Value ([uint32]$proxyBytes.Length)
        [System.Array]::Copy($proxyBytes, 0, $newBlob, 16, $proxyBytes.Length)
        if ($restLength -gt 0) {
            [System.Array]::Copy($blob, $restStart, $newBlob, 16 + $proxyBytes.Length, $restLength)
        }
        $blobMode = 'rewritten'
    }
    else {
        $newBlob = [byte[]]$blob.Clone()
        Set-UInt32 -Bytes $newBlob -Offset 4 -Value $counter
        Set-UInt32 -Bytes $newBlob -Offset 8 -Value ([uint32]$flags)
        $blobMode = 'flag-only'
    }
    Set-ItemProperty -LiteralPath $connectionsPath -Name 'DefaultConnectionSettings' -Value $newBlob -Type Binary
}
elseif ($null -ne $blob) {
    $blobMode = 'left-unchanged'
}

[pscustomobject]@{
    result = 'broken'
    facts  = [ordered]@{
        proxy_server = $proxyServer
        blob         = $blobMode
    }
}
