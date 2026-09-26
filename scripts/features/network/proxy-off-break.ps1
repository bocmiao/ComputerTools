# Feature: network.proxy-off -- break (used only by the automated round-trip test)
# Produces the "leftover proxy" state that network.proxy-dead reports as dead:
#   Internet Settings\ProxyEnable = 1, ProxyServer = 127.0.0.1:9 (discard port;
#   normally nothing listens there)
#   every connection blob under Internet Settings\Connections that already exists
#   (DefaultConnectionSettings and each dial-up / VPN connection; not
#   SavedLegacySettings or WinHttpSettings):
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
$ignoredValues = @('SavedLegacySettings', 'WinHttpSettings')

# Every per-connection settings blob under Connections: name -> byte[].
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
    return , $result
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

# The blob with the manual proxy switched on and pointing at $proxyServer.
function Get-BrokenBlob {
    param([byte[]]$Blob)
    $flags = [System.BitConverter]::ToUInt32($Blob, 8) -bor $proxyFlag
    $counter = Get-NextCounter ([System.BitConverter]::ToUInt32($Blob, 4))

    $oldLength = -1
    if ($Blob.Length -ge 16) {
        $oldLength = [long][System.BitConverter]::ToUInt32($Blob, 12)
    }
    if (($oldLength -ge 0) -and ((16 + $oldLength) -le $Blob.Length)) {
        $proxyBytes = [System.Text.Encoding]::ASCII.GetBytes($proxyServer)
        $restStart = [int](16 + $oldLength)
        $restLength = $Blob.Length - $restStart
        $newBlob = New-Object byte[] (16 + $proxyBytes.Length + $restLength)
        [System.Array]::Copy($Blob, 0, $newBlob, 0, 4)
        Set-UInt32 -Bytes $newBlob -Offset 4 -Value $counter
        Set-UInt32 -Bytes $newBlob -Offset 8 -Value ([uint32]$flags)
        Set-UInt32 -Bytes $newBlob -Offset 12 -Value ([uint32]$proxyBytes.Length)
        [System.Array]::Copy($proxyBytes, 0, $newBlob, 16, $proxyBytes.Length)
        if ($restLength -gt 0) {
            [System.Array]::Copy($Blob, $restStart, $newBlob, 16 + $proxyBytes.Length, $restLength)
        }
        return [pscustomobject]@{ Mode = 'rewritten'; Blob = $newBlob }
    }
    $copy = [byte[]]$Blob.Clone()
    Set-UInt32 -Bytes $copy -Offset 4 -Value $counter
    Set-UInt32 -Bytes $copy -Offset 8 -Value ([uint32]$flags)
    return [pscustomobject]@{ Mode = 'flag-only'; Blob = $copy }
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

$blobs = Get-ConnectionBlobs $connectionsPath
$modes = New-Object System.Collections.Generic.List[string]
foreach ($name in @($blobs.Keys)) {
    $broken = Get-BrokenBlob -Blob $blobs[$name]
    Set-ItemProperty -LiteralPath $connectionsPath -Name $name -Value ([byte[]]$broken.Blob) -Type Binary
    $modes.Add(('{0}={1}' -f $name, $broken.Mode))
}

[pscustomobject]@{
    result = 'broken'
    facts  = [ordered]@{
        proxy_server = $proxyServer
        connections  = ($modes -join ', ')
    }
}
