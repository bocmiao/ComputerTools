# Feature: update.enable-services -- detect
# Is any update service that the fix can turn back on set to Disabled?
#   applied      none of them is Disabled (missing ones are left to the check)
#   not-applied  at least one is Disabled
# Read-only.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block update-services: identical in checks/update/blocked.ps1 and features/update/enable-services-*.ps1 (medkit-data check compares them) ----
# The services Windows Update needs, with the start type Windows gives each of
# them (the names sc.exe uses: auto, delayed-auto, demand). "Optimizer" tools
# set them to Disabled. BITS switches itself between demand and delayed-auto,
# and Windows Modules Installer (TrustedInstaller) to auto while an update
# waits for a restart; both are fine. Windows Update Medic (WaaSMedicSvc) and
# Delivery Optimization (DoSvc) are protected services: the service manager
# refuses to change them even for administrators, so they have no start type
# here, and the fix leaves them alone (the check reports them).
$updateServiceDefaults = [ordered]@{
    'wuauserv'         = 'demand'
    'UsoSvc'           = 'delayed-auto'
    'BITS'             = 'demand'
    'CryptSvc'         = 'auto'
    'TrustedInstaller' = 'demand'
    'DoSvc'            = ''
    'WaaSMedicSvc'     = ''
}

# How a service starts, as the service manager saved it in the registry:
# auto, delayed-auto, demand, disabled, other, or missing.
function Get-ServiceStart {
    param([string]$Name)
    $key = 'HKLM:\SYSTEM\CurrentControlSet\Services\' + $Name
    if (-not (Test-Path -LiteralPath $key)) {
        return 'missing'
    }
    $properties = Get-ItemProperty -LiteralPath $key
    $start = $properties.PSObject.Properties['Start']
    if ($null -eq $start) {
        return 'other'
    }
    switch ([int64]$start.Value) {
        2 {
            $delayed = $properties.PSObject.Properties['DelayedAutostart']
            if (($null -ne $delayed) -and ([int64]$delayed.Value -eq 1)) {
                return 'delayed-auto'
            }
            return 'auto'
        }
        3 {
            return 'demand'
        }
        4 {
            return 'disabled'
        }
        default {
            return 'other'
        }
    }
}
# ---- end of shared block update-services ----

$state = 'applied'
foreach ($name in $updateServiceDefaults.Keys) {
    if (($updateServiceDefaults[$name].Length -gt 0) -and ((Get-ServiceStart $name) -eq 'disabled')) {
        $state = 'not-applied'
    }
}
[pscustomobject]@{ state = $state }
