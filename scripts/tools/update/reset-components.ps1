# Tool: update.reset-components (action)
# Resets the Windows Update download cache the way Microsoft's manual steps
# do, without deleting anything: stops the services that use it, renames
# %SystemRoot%\SoftwareDistribution to SoftwareDistribution.old-<time>, and
# starts the services that were running again. Windows makes a new, empty
# folder the next time it checks for updates. The update history shown in
# Settings starts over (the old one stays in the renamed folder).
# Not done: catroot2 (stopping Cryptographic Services stops the services that
# depend on it), resetting permissions, registering DLLs.
# Refuses (nothing changed) when:
#   restart-first  an update waits for a restart (renaming now could break it)
#   installing     Windows is installing updates right now (TiWorker runs)
#   disabled       Windows Update is Disabled (fix: update.enable-services)
# Results: reset / restart-first / installing / disabled / not-stopped (a
# service did not stop within $stopSeconds; started again, nothing renamed) /
# in-use (the folder could not be renamed; services started again) /
# no-cache (there is no SoftwareDistribution folder to rename).
# Facts: old_folder (the full path of the renamed folder, or '').

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$stopSeconds = 30
# Update Orchestrator first: it starts Windows Update again when it runs.
$services = @('UsoSvc', 'wuauserv', 'BITS', 'DoSvc')
$cache = Join-Path $env:SystemRoot 'SoftwareDistribution'

function Test-RegistryKey {
    param([string]$Path)
    return (Test-Path -LiteralPath $Path)
}

function Get-Result {
    param([string]$Code, [string]$Folder = '')
    return [pscustomobject]@{ result = $Code; facts = [ordered]@{ old_folder = $Folder } }
}

if ((Test-RegistryKey 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending') -or
    (Test-RegistryKey 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired')) {
    return Get-Result 'restart-first'
}
if (@(Get-Process -Name 'TiWorker' -ErrorAction SilentlyContinue).Count -gt 0) {
    return Get-Result 'installing'
}
$wu = Get-Service -Name 'wuauserv'
if ([string]$wu.StartType -eq 'Disabled') {
    return Get-Result 'disabled'
}

# Stop what runs; remember it to start it again.
$stopped = New-Object System.Collections.Generic.List[string]
$allStopped = $true
foreach ($name in $services) {
    $service = Get-Service -Name $name -ErrorAction SilentlyContinue
    if (($null -eq $service) -or ($service.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Stopped)) {
        continue
    }
    try {
        $service.Stop()
        $service.WaitForStatus([System.ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds($stopSeconds))
        $stopped.Add($name)
    }
    catch {
        Write-Verbose ('{0} did not stop: {1}' -f $name, $_.Exception.Message)
        # Update Orchestrator may refuse to stop; the others must stop.
        if ($name -ne 'UsoSvc') {
            $allStopped = $false
            break
        }
    }
}

$code = 'not-stopped'
$folder = ''
if ($allStopped -and (-not (Test-Path -LiteralPath $cache))) {
    $code = 'no-cache'
}
elseif ($allStopped) {
    $name = 'SoftwareDistribution.old-{0}' -f (Get-Date).ToString('yyyyMMdd-HHmmss')
    try {
        Rename-Item -LiteralPath $cache -NewName $name
        $code = 'reset'
        $folder = Join-Path $env:SystemRoot $name
    }
    catch {
        Write-Verbose ('SoftwareDistribution could not be renamed: {0}' -f $_.Exception.Message)
        $code = 'in-use'
    }
}

# Start again what was running, in the opposite order.
for ($i = $stopped.Count - 1; $i -ge 0; $i--) {
    try {
        Start-Service -Name $stopped[$i]
    }
    catch {
        Write-Verbose ('{0} could not be started again: {1}' -f $stopped[$i], $_.Exception.Message)
    }
}
Get-Result $code $folder
