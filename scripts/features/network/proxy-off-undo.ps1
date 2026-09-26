# Feature: network.proxy-off -- undo
# Restores exactly what proxy-off-run.ps1 recorded before its change.
# -Before is the JSON of that record: { "proxy_enable": <int or null>, "dcs": <hex string or null> }
#   proxy_enable null -> delete Internet Settings\ProxyEnable (if present)
#   proxy_enable n    -> write DWORD n
#   dcs null          -> delete Connections\DefaultConnectionSettings (if present);
#                        the Connections key is never created for this
#   dcs "hex"         -> write the bytes back unchanged as REG_BINARY (the Connections
#                        key is created only if it has disappeared in the meantime)
# The whole input is validated before anything is changed.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:',
    [Parameter(Mandatory = $true)]
    [string]$Before
)

$ErrorActionPreference = 'Stop'

function Test-RegistryValue {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }
    $item = Get-ItemProperty -LiteralPath $Path
    if ($null -eq $item) {
        return $false
    }
    return ($null -ne $item.PSObject.Properties[$Name])
}

function ConvertFrom-HexString {
    param([string]$Hex)
    if ((($Hex.Length % 2) -ne 0) -or ($Hex -notmatch '^[0-9A-Fa-f]*$')) {
        throw 'The saved DefaultConnectionSettings value is not a hex string'
    }
    $bytes = New-Object byte[] ($Hex.Length / 2)
    for ($i = 0; $i -lt $bytes.Length; $i++) {
        $bytes[$i] = [System.Convert]::ToByte($Hex.Substring($i * 2, 2), 16)
    }
    # The unary comma keeps the byte[] from being unrolled into single bytes.
    return , $bytes
}

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$settingsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$connectionsPath = Join-Path $UserHive 'Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections'

# Validate the saved state first.
$state = $Before | ConvertFrom-Json
if ($null -eq $state) {
    throw 'No saved state to restore'
}
$names = @($state.PSObject.Properties | ForEach-Object { $_.Name })
if (($names -notcontains 'proxy_enable') -or ($names -notcontains 'dcs')) {
    throw 'The saved state does not have the expected fields (proxy_enable, dcs)'
}
$enableValue = $null
if ($null -ne $state.proxy_enable) {
    $enableValue = [int]$state.proxy_enable
}
$blob = $null
if ($null -ne $state.dcs) {
    $blob = ConvertFrom-HexString ([string]$state.dcs)
}

# ProxyEnable.
if ($null -eq $enableValue) {
    if (Test-RegistryValue -Path $settingsPath -Name 'ProxyEnable') {
        Remove-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable'
    }
}
else {
    Set-ItemProperty -LiteralPath $settingsPath -Name 'ProxyEnable' -Value $enableValue -Type DWord
}

# DefaultConnectionSettings.
if ($null -eq $blob) {
    if (Test-RegistryValue -Path $connectionsPath -Name 'DefaultConnectionSettings') {
        Remove-ItemProperty -LiteralPath $connectionsPath -Name 'DefaultConnectionSettings'
    }
}
else {
    if (-not (Test-Path -LiteralPath $connectionsPath)) {
        $null = New-Item -Path $connectionsPath -Force
    }
    Set-ItemProperty -LiteralPath $connectionsPath -Name 'DefaultConnectionSettings' -Value $blob -Type Binary
}

$blobLength = $null
if ($null -ne $blob) {
    $blobLength = $blob.Length
}

[pscustomobject]@{
    restored = [ordered]@{
        proxy_enable = $enableValue
        dcs_bytes    = $blobLength
    }
}
