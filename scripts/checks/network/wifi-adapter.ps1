# Check: network.wifi-adapter
# Is the WiFi adapter disabled ("Disable" in Network Connections or in Device
# Manager)? Then Windows shows no WiFi at all: no Wi-Fi page in Settings, no
# wireless networks in the network list, only Ethernet. It is also the verify
# check of network.enable-wifi-adapter, which enables it again.
# Read-only. Result codes: ok (a WiFi adapter is enabled), disabled (there
# are WiFi adapters and all of them are disabled), none (no WiFi adapter at
# all: many desktops have none; a WiFi card without a working driver is not a
# network adapter yet, hardware.device-problems reports it). Throws when
# Get-NetAdapter does not work here.
# Facts: wifi_count (enabled WiFi adapters), disabled_count, adapter (the
# model of the first enabled one, else of the first disabled one; model names
# come from the driver and say nothing about the user).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block wifi-adapter: identical in checks/network/wifi-adapter.ps1 and features/network/wifi-adapter-*.ps1 (medkit-data check compares them) ----
# Wireless (WiFi) network adapters, as Network Connections lists them
# (Get-NetAdapter, hidden ones left out). Physical: HardwareInterface or
# ConnectorPresent, and not EndPointInterface (MSFT_NetAdapter). WiFi:
# NdisPhysicalMedium 9 (native 802.11) or 1 (wireless LAN), InterfaceType 71
# (IEEE 802.11), or PhysicalMediaType '802.11' / 'Wireless LAN'. A disabled
# adapter may not report these, so the driver's own *PhysicalMediaType and
# *IfType under the network adapter class key (the subkey whose
# NetCfgInstanceId is the adapter's DeviceID) count too, and the model name
# is the last resort (Wi-Fi, WLAN, 802.11, wireless LAN / network / adapter).
# Bluetooth, mobile broadband (Wireless WAN) and Wi-Fi Direct adapters are
# not WiFi. Disabled: State 3 or Status 'Disabled' (checked first: a disabled
# adapter can also read as not present). Not present (InterfaceOperationalStatus
# 6, Status 'Not Present', e.g. an unplugged USB adapter) ones are left out.
$wifiClassKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}'
$wifiNamePattern = '(?i)wi-?fi|\bwlan\b|802\.11|wireless\s+(lan|network|adapter)'
$notWifiNamePattern = '(?i)bluetooth|\bwwan\b|wireless\s+wan|mobile\s+broadband|wi-?fi\s+direct'
$script:wifiDriverMedia = $null

function Get-WifiText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

# A whole number, or -1 when the value is missing or not a number.
function Get-WifiNumber {
    param($Value)
    $n = [long]0
    if (($null -ne $Value) -and [long]::TryParse(([string]$Value).Trim(), [ref]$n)) {
        return $n
    }
    return [long]-1
}

function Test-WifiTrue {
    param($Value)
    if ($null -eq $Value) {
        return $false
    }
    if ($Value -is [bool]) {
        return $Value
    }
    $t = (Get-WifiText $Value).ToLowerInvariant()
    return (($t -eq 'true') -or ($t -eq '1'))
}

# The driver's *PhysicalMediaType and *IfType of every network adapter, by
# NetCfgInstanceId (lower case, with braces); read once, when first needed.
function Get-WifiDriverMedia {
    if ($null -ne $script:wifiDriverMedia) {
        return $script:wifiDriverMedia
    }
    $map = @{}
    # Some subkeys (Properties) cannot be opened; the others are enough.
    foreach ($key in @(Get-ChildItem -LiteralPath $wifiClassKey -ErrorAction SilentlyContinue)) {
        $item = $null
        try {
            $item = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction Stop
        }
        catch {
            continue
        }
        $id = (Get-WifiText $item.NetCfgInstanceId).ToLowerInvariant()
        if ($id.Length -gt 0) {
            $map[$id] = @{ Medium = (Get-WifiNumber $item.'*PhysicalMediaType'); IfType = (Get-WifiNumber $item.'*IfType') }
        }
    }
    $script:wifiDriverMedia = $map
    return $map
}

function Test-WifiAdapter {
    param($Adapter)
    $model = Get-WifiText $Adapter.InterfaceDescription
    $mediaText = Get-WifiText $Adapter.PhysicalMediaType
    $medium = Get-WifiNumber $Adapter.NdisPhysicalMedium
    if (($medium -eq 10) -or ($medium -eq 8) -or ($model -match $notWifiNamePattern) -or ($mediaText -match 'Bluetooth|Wireless WAN')) {
        return $false
    }
    if (($medium -eq 9) -or ($medium -eq 1) -or ((Get-WifiNumber $Adapter.InterfaceType) -eq 71) -or ($mediaText -match '802\.11|Wireless LAN')) {
        return $true
    }
    $id = (Get-WifiText $Adapter.DeviceID).ToLowerInvariant()
    $media = Get-WifiDriverMedia
    if (($id.Length -gt 0) -and $media.ContainsKey($id)) {
        $driver = $media[$id]
        if (($driver.Medium -eq 9) -or ($driver.Medium -eq 1) -or ($driver.IfType -eq 71)) {
            return $true
        }
        # The driver says what it is (Ethernet, ...): trust it over the name.
        if (($driver.Medium -gt 0) -or ($driver.IfType -gt 0)) {
            return $false
        }
    }
    return ($model -match $wifiNamePattern)
}

# The physical WiFi adapters as @{ Id (DeviceID, a GUID); Model
# (InterfaceDescription); Disabled; Adapter (the Get-NetAdapter object) }, or
# $null when Get-NetAdapter does not work here.
function Get-WifiAdapters {
    $all = $null
    try {
        $all = @(Get-NetAdapter -ErrorAction Stop)
    }
    catch {
        return $null
    }
    $list = New-Object System.Collections.Generic.List[object]
    foreach ($adapter in $all) {
        if ($null -eq $adapter) {
            continue
        }
        if ((Test-WifiTrue $adapter.EndPointInterface) -or (-not ((Test-WifiTrue $adapter.HardwareInterface) -or (Test-WifiTrue $adapter.ConnectorPresent)))) {
            continue
        }
        $status = Get-WifiText $adapter.Status
        $disabled = (((Get-WifiNumber $adapter.State) -eq 3) -or ($status -eq 'Disabled'))
        if ((-not $disabled) -and (((Get-WifiNumber $adapter.InterfaceOperationalStatus) -eq 6) -or ($status -eq 'Not Present'))) {
            continue
        }
        if (-not (Test-WifiAdapter $adapter)) {
            continue
        }
        $list.Add([pscustomobject]@{
                Id       = (Get-WifiText $adapter.DeviceID).ToLowerInvariant()
                Model    = (Get-WifiText $adapter.InterfaceDescription)
                Disabled = $disabled
                Adapter  = $adapter
            })
    }
    return , $list.ToArray()
}

# Waits (up to $wifiWaitSeconds) until every adapter in $Ids reads back as
# enabled ($Enabled) or as disabled; $true when they all do. Enabling or
# disabling restarts the device, and the adapter can be missing for a moment.
$wifiWaitSeconds = 15

function Wait-WifiState {
    param([string[]]$Ids, [bool]$Enabled)
    $deadline = (Get-Date).AddSeconds($wifiWaitSeconds)
    while ($true) {
        $now = Get-WifiAdapters
        if ($null -ne $now) {
            $done = $true
            foreach ($id in $Ids) {
                $found = @($now | Where-Object { $_.Id -eq $id })
                if (($found.Count -eq 0) -or ($found[0].Disabled -eq $Enabled)) {
                    $done = $false
                }
            }
            if ($done) {
                return $true
            }
        }
        if ((Get-Date) -ge $deadline) {
            return $false
        }
        Start-Sleep -Milliseconds 500
    }
}
# ---- end of shared block wifi-adapter ----

# Used when an adapter has no model name: "wu xian wang ka" (WiFi adapter)
# as \u escapes, because it ends up in the messages; the script stays ASCII.
$unnamedWifi = -join [char[]](0x65E0, 0x7EBF, 0x7F51, 0x5361)

function Get-ModelName {
    param($Wifi)
    if ($Wifi.Model.Length -gt 0) {
        return $Wifi.Model
    }
    return $unnamedWifi
}

$adapters = Get-WifiAdapters
if ($null -eq $adapters) {
    throw 'Get-NetAdapter does not work on this computer'
}
$enabled = @($adapters | Where-Object { -not $_.Disabled })
$disabled = @($adapters | Where-Object { $_.Disabled })
$facts = [ordered]@{ wifi_count = $enabled.Count; disabled_count = $disabled.Count; adapter = '' }

$result = 'none'
if ($enabled.Count -gt 0) {
    $result = 'ok'
    $facts.adapter = Get-ModelName $enabled[0]
}
elseif ($disabled.Count -gt 0) {
    $result = 'disabled'
    $facts.adapter = Get-ModelName $disabled[0]
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
