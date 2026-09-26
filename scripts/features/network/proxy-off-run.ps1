# Feature: network.proxy-off -- run
# Switches the manual system proxy off in both places that hold it:
#   1. Internet Settings\ProxyEnable            -> DWORD 0
#   2. Internet Settings\Connections\DefaultConnectionSettings (REG_BINARY, what
#      WinINet and .NET actually read), if it exists and is at least 12 bytes:
#      clear flag 0x02 in the DWORD at offset 8 and add 1 to the change counter
#      (DWORD at offset 4, wraps at 2^32). The blob is only rewritten when the
#      flag is set.
# ProxyServer, ProxyOverride, AutoConfigURL and SavedLegacySettings are not touched.
# The original state is recorded before anything is changed and returned as
# "before" = { proxy_enable: <int or null>, dcs: <hex string or null> }; the
# undo script gets it back through -Before. "after" has the same shape and is
# read back from the registry after the change.
# Running programs keep their cached proxy settings until they re-read them;
# this script cannot notify them.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$proxyFlag = 2

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

# Writes a little-endian DWORD into the array in place.
function Set-UInt32 {
    param($Bytes, [int]$Offset, [uint32]$Value)
    $b = [System.BitConverter]::GetBytes($Value)
    [System.Array]::Copy($b, 0, $Bytes, $Offset, 4)
}

function Get-State {
    param([string]$SettingsPath, [string]$ConnectionsPath)
    $enable = Get-RegistryValue -Path $SettingsPath -Name 'ProxyEnable'
    $enableValue = $null
    if ($null -ne $enable) {
        $enableValue = [int]$enable
    }
    $blob = Get-RegistryValue -Path $ConnectionsPath -Name 'DefaultConnectionSettings'
    if (($null -ne $blob) -and (-not ($blob -is [byte[]]))) {
        throw 'DefaultConnectionSettings is not a binary value'
    }
    return [pscustomobject]@{ ProxyEnable = $enableValue; Blob = $blob }
}

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

if (-not (Test-Path -LiteralPath $settingsPath)) {
    throw ('The Internet Settings key does not exist: {0}' -f $settingsPath)
}

# 1. Record the original state. Nothing has been changed yet, so any error
#    above or here leaves the registry as it was.
$old = Get-State -SettingsPath $settingsPath -ConnectionsPath $connectionsPath
$before = [ordered]@{
    proxy_enable = $old.ProxyEnable
    dcs          = ConvertTo-HexString $old.Blob
}

# 2. Change.
Set-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable' -Value 0 -Type DWord

$blobUpdated = $false
if (($null -ne $old.Blob) -and ($old.Blob.Length -ge 12)) {
    $flags = [System.BitConverter]::ToUInt32($old.Blob, 8)
    if (($flags -band $proxyFlag) -ne 0) {
        $newBlob = [byte[]]$old.Blob.Clone()
        Set-UInt32 -Bytes $newBlob -Offset 8 -Value ([uint32]($flags - $proxyFlag))
        $counter = [System.BitConverter]::ToUInt32($old.Blob, 4)
        if ($counter -eq [uint32]::MaxValue) {
            $next = [uint32]0
        }
        else {
            $next = [uint32]($counter + 1)
        }
        Set-UInt32 -Bytes $newBlob -Offset 4 -Value $next
        Set-ItemProperty -LiteralPath $connectionsPath -Name 'DefaultConnectionSettings' -Value $newBlob -Type Binary
        $blobUpdated = $true
    }
}

# 3. Read back what is there now.
$new = Get-State -SettingsPath $settingsPath -ConnectionsPath $connectionsPath
$after = [ordered]@{
    proxy_enable = $new.ProxyEnable
    dcs          = ConvertTo-HexString $new.Blob
}

[pscustomobject]@{
    before = $before
    after  = $after
    facts  = [ordered]@{
        legacy_was_enabled = ($old.ProxyEnable -eq 1)
        blob_present       = ($null -ne $old.Blob)
        blob_updated       = $blobUpdated
    }
}
