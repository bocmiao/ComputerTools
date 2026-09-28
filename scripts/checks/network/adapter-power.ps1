# Check: network.adapter-power
# May Windows turn the network adapter off to save power? With "Allow the
# computer to turn off this device to save power" ticked (the default for
# most adapters), some WiFi and Ethernet adapters drop the connection every
# now and then, or do not come back after sleep. Read-only.
# Result codes: allowed (advice; fixed by network.adapter-power-off) / off
# (ok) / none (nothing connected, or no adapter has this setting; na).
# Facts: checked_count, allowed_count, adapter (the model of the first
# adapter that allows it; model names come from the driver).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block adapter-power: identical in checks/network/adapter-power.ps1 and features/network/adapter-power-*.ps1 (medkit-data check compares them) ----
# "Allow the computer to turn off this device to save power" (Device Manager,
# the adapter's Power Management tab) of the network adapters that are
# connected now: physical (HardwareInterface or ConnectorPresent, not
# EndPointInterface) and up (Status Up / InterfaceOperationalStatus 1).
# Read with Get-NetAdapterPowerManagement: AllowComputerToTurnOffDevice is
# Enabled, Disabled or Unsupported (MSFT_NetAdapterPowerManagementSettingData).
# Changed the way Sophia Script does it (MIT): set the property on that
# object and pass it to Set-NetAdapterPowerManagement, with -NoRestart so the
# adapter is not restarted and the network does not drop now; the change is
# used after a restart.
function Get-NicText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

function Test-NicTrue {
    param($Value)
    if ($null -eq $Value) {
        return $false
    }
    if ($Value -is [bool]) {
        return $Value
    }
    $t = (Get-NicText $Value).ToLowerInvariant()
    return (($t -eq 'true') -or ($t -eq '1'))
}

# enabled / disabled / unsupported. The CIM property is a number (2 Enabled,
# 1 Disabled, 0 Unsupported); PowerShell shows it by name.
function Get-AllowState {
    param($Value)
    $t = (Get-NicText $Value).ToLowerInvariant()
    if (($t -eq 'enabled') -or ($t -eq '2')) {
        return 'enabled'
    }
    if (($t -eq 'disabled') -or ($t -eq '1')) {
        return 'disabled'
    }
    return 'unsupported'
}

# The connected physical adapters as @{ Id (DeviceID, lower case); Model
# (InterfaceDescription); Allow; Settings (the power management object) },
# or $null when Get-NetAdapter does not work here. Adapters whose power
# management cannot be read are left out.
function Get-AdapterPower {
    $adapters = $null
    try {
        $adapters = @(Get-NetAdapter -ErrorAction Stop)
    }
    catch {
        return $null
    }
    $list = New-Object System.Collections.Generic.List[object]
    foreach ($adapter in $adapters) {
        if (($null -eq $adapter) -or (Test-NicTrue $adapter.EndPointInterface) -or (-not ((Test-NicTrue $adapter.HardwareInterface) -or (Test-NicTrue $adapter.ConnectorPresent)))) {
            continue
        }
        $up = ((Get-NicText $adapter.Status) -eq 'Up') -or ((Get-NicText $adapter.InterfaceOperationalStatus) -eq '1')
        if (-not $up) {
            continue
        }
        $settings = $null
        try {
            $settings = Get-NetAdapterPowerManagement -Name $adapter.Name -ErrorAction Stop
        }
        catch {
            continue
        }
        if ($null -eq $settings) {
            continue
        }
        $list.Add([pscustomobject]@{
                Id       = (Get-NicText $adapter.DeviceID).ToLowerInvariant()
                Model    = (Get-NicText $adapter.InterfaceDescription)
                Allow    = (Get-AllowState $settings.AllowComputerToTurnOffDevice)
                Settings = $settings
            })
    }
    return , $list.ToArray()
}

# Sets AllowComputerToTurnOffDevice of one adapter to Enabled or Disabled
# (not restarting the adapter).
function Set-AllowState {
    param($Adapter, [string]$State)
    $settings = $Adapter.Settings
    $settings.AllowComputerToTurnOffDevice = $State
    $settings | Set-NetAdapterPowerManagement -NoRestart -ErrorAction Stop
}
# ---- end of shared block adapter-power ----

$adapters = Get-AdapterPower
$facts = [ordered]@{ checked_count = 0; allowed_count = 0; adapter = '' }
$result = 'none'
if ($null -ne $adapters) {
    $known = @($adapters | Where-Object { $_.Allow -ne 'unsupported' })
    $allowed = @($known | Where-Object { $_.Allow -eq 'enabled' })
    $facts.checked_count = $known.Count
    $facts.allowed_count = $allowed.Count
    if ($allowed.Count -gt 0) {
        $result = 'allowed'
        $facts.adapter = $allowed[0].Model
    }
    elseif ($known.Count -gt 0) {
        $result = 'off'
    }
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
