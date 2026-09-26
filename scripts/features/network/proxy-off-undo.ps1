# Feature: network.proxy-off -- undo
# Puts back exactly the bits that proxy-off-run.ps1 changed, and nothing else.
# -Before is the JSON that the run script recorded:
#   { "proxy_enable": <int or null>, "legacy_changed": <bool>,
#     "connections": { "<value name>": "<hex of the original blob>" } }
# (Records written by earlier versions look like { "proxy_enable", "dcs" }; they
# are treated as legacy_changed = true and connections = { DefaultConnectionSettings: dcs }.)
#   ProxyEnable (only when legacy_changed): written back as DWORD, or deleted if
#     it did not exist before
#   each recorded connection blob:
#     - still there: only flag 0x02 of the DWORD at offset 8 is set back to what
#       it was and the change counter (offset 4) is increased by 1; every other
#       byte keeps its current value, so settings changed later (proxy address,
#       bypass list, automatic configuration script, auto-detect) survive
#     - gone, or no longer a usable binary value: the whole original blob is
#       written back (the Connections key is created only if it has disappeared)
# The whole input is validated before anything is changed. If a write fails
# half-way, the values already changed by this script are put back and the error
# is thrown.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:',
    [Parameter(Mandatory = $true)]
    [string]$Before
)

$ErrorActionPreference = 'Stop'

$proxyFlag = 2
$lanValue = 'DefaultConnectionSettings'

function ConvertFrom-HexString {
    param([string]$Hex)
    if ((($Hex.Length % 2) -ne 0) -or ($Hex -notmatch '^[0-9A-Fa-f]*$')) {
        throw 'A saved connection setting is not a hex string'
    }
    $bytes = New-Object byte[] ($Hex.Length / 2)
    for ($i = 0; $i -lt $bytes.Length; $i++) {
        $bytes[$i] = [System.Convert]::ToByte($Hex.Substring($i * 2, 2), 16)
    }
    # The unary comma keeps the byte[] from being unrolled into single bytes.
    return , $bytes
}

# @{ Exists; Value; Kind } of a registry value; the key may be missing.
function Get-ValueState {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path -LiteralPath $Path)) {
        return [pscustomobject]@{ Exists = $false; Value = $null; Kind = '' }
    }
    $key = Get-Item -LiteralPath $Path
    if (@($key.GetValueNames()) -notcontains $Name) {
        return [pscustomobject]@{ Exists = $false; Value = $null; Kind = '' }
    }
    return [pscustomobject]@{ Exists = $true; Value = $key.GetValue($Name); Kind = [string]$key.GetValueKind($Name) }
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
    return , $copy
}

# Writes (or deletes, when State.Exists is false) a value back to a recorded
# state, with its original registry type. Used only to roll back after an error.
function Restore-ValueState {
    param([string]$Path, [string]$Name, $State)
    if ($State.Exists) {
        Set-ItemProperty -LiteralPath $Path -Name $Name -Value $State.Value -Type $State.Kind -ErrorAction SilentlyContinue
    }
    elseif (Test-Path -LiteralPath $Path) {
        Remove-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction SilentlyContinue
    }
}

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

# 1. Validate the saved state and work out every write before changing anything.
$state = $Before | ConvertFrom-Json
if ($null -eq $state) {
    throw 'No saved state to restore'
}
$names = @($state.PSObject.Properties | ForEach-Object { $_.Name })
if ($names -notcontains 'proxy_enable') {
    throw 'The saved state does not have the expected field proxy_enable'
}
$enableValue = $null
if ($null -ne $state.proxy_enable) {
    $enableValue = [int]$state.proxy_enable
}

$saved = [ordered]@{}
if ($names -contains 'connections') {
    $legacyChanged = [bool]$state.legacy_changed
    if ($null -ne $state.connections) {
        foreach ($p in @($state.connections.PSObject.Properties)) {
            if ($null -ne $p.Value) {
                $saved[$p.Name] = ConvertFrom-HexString ([string]$p.Value)
            }
        }
    }
}
elseif ($names -contains 'dcs') {
    # Record from an earlier version of the run script.
    $legacyChanged = $true
    if ($null -ne $state.dcs) {
        $saved[$lanValue] = ConvertFrom-HexString ([string]$state.dcs)
    }
}
else {
    throw 'The saved state does not have the expected fields (connections)'
}
foreach ($name in @($saved.Keys)) {
    if (([byte[]]$saved[$name]).Length -lt 12) {
        throw ('The saved setting of connection {0} is too short' -f $name)
    }
}

$writes = New-Object System.Collections.Generic.List[object]
foreach ($name in @($saved.Keys)) {
    $original = [byte[]]$saved[$name]
    $wasOn = ((([System.BitConverter]::ToUInt32($original, 8)) -band $proxyFlag) -ne 0)
    $current = Get-ValueState -Path $connectionsPath -Name $name
    if ($current.Exists -and ($current.Value -is [byte[]]) -and ($current.Value.Length -ge 12)) {
        $isOn = ((([System.BitConverter]::ToUInt32([byte[]]$current.Value, 8)) -band $proxyFlag) -ne 0)
        if ($isOn -ne $wasOn) {
            $writes.Add([pscustomobject]@{ Name = $name; Value = (Set-ProxyFlag -Blob $current.Value -On $wasOn); Previous = $current })
        }
    }
    else {
        $writes.Add([pscustomobject]@{ Name = $name; Value = $original; Previous = $current })
    }
}
$previousEnable = Get-ValueState -Path $settingsPath -Name 'ProxyEnable'

# 2. Change; put back what this script already changed if a write fails.
$enableWritten = $false
$done = New-Object System.Collections.Generic.List[object]
try {
    if ($legacyChanged) {
        if ($null -eq $enableValue) {
            if ($previousEnable.Exists) {
                Remove-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable'
                $enableWritten = $true
            }
        }
        else {
            Set-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable' -Value $enableValue -Type DWord
            $enableWritten = $true
        }
    }
    foreach ($w in $writes) {
        if (-not (Test-Path -LiteralPath $connectionsPath)) {
            # No -Force: on an existing registry key, New-Item -Force deletes the key
            # with all its values and creates an empty one.
            $null = New-Item -Path $connectionsPath
        }
        Set-ItemProperty -LiteralPath $connectionsPath -Name $w.Name -Value ([byte[]]$w.Value) -Type Binary
        $done.Add($w)
    }
}
catch {
    $failure = $_
    if ($enableWritten) {
        Restore-ValueState -Path $settingsPath -Name 'ProxyEnable' -State $previousEnable
    }
    foreach ($w in $done) {
        Restore-ValueState -Path $connectionsPath -Name $w.Name -State $w.Previous
    }
    throw $failure
}

[pscustomobject]@{
    restored = [ordered]@{
        proxy_enable        = $enableValue
        legacy_changed      = $legacyChanged
        connections         = $saved.Count
        connections_written = $done.Count
    }
}
