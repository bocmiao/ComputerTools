# Feature: network.enable-wifi-adapter -- break (tests only)
# Disables every enabled WiFi adapter, the way "Disable" in Network
# Connections does, and waits until they read back as disabled. Throws when
# there is no enabled WiFi adapter.

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

$adapters = Get-WifiAdapters
$enabled = @($adapters | Where-Object { ($null -ne $_) -and (-not $_.Disabled) })
if ($enabled.Count -eq 0) {
    throw 'there is no enabled WiFi adapter to disable'
}
foreach ($wifi in $enabled) {
    Disable-NetAdapter -InputObject $wifi.Adapter -Confirm:$false -ErrorAction Stop
}
if (-not (Wait-WifiState -Ids @($enabled | ForEach-Object { $_.Id }) -Enabled $false)) {
    throw 'the WiFi adapter is still enabled after Disable-NetAdapter'
}

[pscustomobject]@{ result = 'ok' }
