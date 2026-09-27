# Feature: update.enable-services -- undo
# Sets the services that were Disabled before the run script (-Before) back
# to Disabled, if they still start the way the run script set them (the
# Windows default). A service changed again since then is left as it is.
# Services that are running keep running until they stop or the PC restarts.

[CmdletBinding()]
param(
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block update-services: identical in checks/update/blocked.ps1 and features/update/enable-services-*.ps1 (medkit-data check compares them) ----
# The services Windows Update needs, with the start type Windows gives each of
# them (the names sc.exe uses: auto, delayed-auto, demand). "Optimizer" tools
# set them to Disabled. Windows Update Medic (WaaSMedicSvc) is checked as well,
# but Windows does not let administrators change it: it has no start type
# here, and the fix leaves it alone.
$updateServiceDefaults = [ordered]@{
    'wuauserv'         = 'demand'
    'UsoSvc'           = 'delayed-auto'
    'BITS'             = 'demand'
    'CryptSvc'         = 'auto'
    'TrustedInstaller' = 'demand'
    'DoSvc'            = 'delayed-auto'
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

# ---- shared block update-services-sc: identical in features/update/enable-services-run.ps1 and -undo.ps1 (medkit-data check compares them) ----
# Sets how a service starts through the service manager (sc.exe, absolute
# path; its output is localized, so only the exit code is read). Returns the
# exit code.
$sc = Join-Path $env:SystemRoot 'System32\sc.exe'

function Set-ServiceStart {
    param([string]$Name, [string]$Start)
    $ErrorActionPreference = 'Continue'
    $null = & $sc 'config' $Name 'start=' $Start 2>&1
    return $LASTEXITCODE
}
# ---- end of shared block update-services-sc ----

$recorded = ConvertFrom-Json -InputObject $Before
foreach ($name in $updateServiceDefaults.Keys) {
    if (($updateServiceDefaults[$name].Length -eq 0) -or ([string]$recorded.$name -ne 'disabled')) {
        continue
    }
    if ((Get-ServiceStart $name) -ne $updateServiceDefaults[$name]) {
        continue
    }
    $exitCode = Set-ServiceStart $name 'disabled'
    if ($exitCode -ne 0) {
        throw ('sc.exe config {0} start= disabled failed (exit code {1})' -f $name, $exitCode)
    }
}
[pscustomobject]@{ result = 'ok' }
