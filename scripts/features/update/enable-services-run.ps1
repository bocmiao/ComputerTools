# Feature: update.enable-services -- run (and prepare)
# Sets the update services that are Disabled back to the start type Windows
# gives them (see the shared block), through the service manager. Services
# that are not Disabled are left as they are.
# -Prepare: returns before = { <service>: start } for every service the fix
#   can change (auto, delayed-auto, demand, disabled, other or missing).
# Run: -Before is that JSON. Returns skipped (nothing changed) when any of
#   them changed since, or when none is Disabled. Otherwise changes each
#   Disabled one and reads it back: it must now start the Windows way, or the
#   script throws (the engine then runs the undo script). Then the ones that
#   start by themselves (auto, delayed-auto) are started, best effort; the
#   others start when Windows Update needs them.

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = ''
)

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

function Get-Snapshot {
    $snapshot = [ordered]@{}
    foreach ($name in $updateServiceDefaults.Keys) {
        if ($updateServiceDefaults[$name].Length -gt 0) {
            $snapshot[$name] = Get-ServiceStart $name
        }
    }
    return $snapshot
}

$snapshot = Get-Snapshot
if ($Prepare) {
    return [pscustomobject]@{ before = $snapshot }
}

$recorded = ConvertFrom-Json -InputObject $Before
$disabled = @()
foreach ($name in $snapshot.Keys) {
    if ([string]$recorded.$name -ne $snapshot[$name]) {
        return [pscustomobject]@{ skipped = $true }
    }
    if ($snapshot[$name] -eq 'disabled') {
        $disabled += $name
    }
}
if ($disabled.Count -eq 0) {
    return [pscustomobject]@{ skipped = $true }
}

foreach ($name in $disabled) {
    $start = $updateServiceDefaults[$name]
    $exitCode = Set-ServiceStart $name $start
    if ($exitCode -ne 0) {
        throw ('sc.exe config {0} start= {1} failed (exit code {2})' -f $name, $start, $exitCode)
    }
    if ((Get-ServiceStart $name) -ne $start) {
        throw ('sc.exe reported success, but {0} does not start as {1}' -f $name, $start)
    }
}
foreach ($name in $disabled) {
    if ($updateServiceDefaults[$name] -ne 'demand') {
        try {
            Start-Service -Name $name
        }
        catch {
            Write-Verbose ('{0} could not be started now: {1}' -f $name, $_.Exception.Message)
        }
    }
}
[pscustomobject]@{ after = (Get-Snapshot) }
