# Tool: update.reset-components (action)
# Resets the Windows Update cache the way Microsoft's "reset Windows Update
# components" steps do, without deleting anything: stops the services that use
# it, renames %SystemRoot%\SoftwareDistribution\DataStore (the update database,
# with the history Settings shows) and ...\Download (downloaded updates) to
# <name>.old-<time>, and starts the services that were running again. Windows
# makes new, empty ones the next time it checks for updates.
# Not done: catroot2 (stopping Cryptographic Services also stops the services
# that depend on it), resetting service permissions (sc sdset: Microsoft warns
# it overwrites them), registering DLLs, winsock reset.
# Stopped: Windows Update and BITS must stop, or nothing is renamed. Update
# Orchestrator (it starts Windows Update again) and Delivery Optimization (a
# protected service that may refuse) are stopped if they let themselves.
# Refuses (nothing changed) when:
#   restart-first  an update waits for a restart: the Component Based Servicing
#                  or Windows Update restart keys exist, or Windows Modules
#                  Installer switched itself to start automatically
#   installing     Windows is installing updates right now (TiWorker runs)
#   disabled       Windows Update is Disabled (fix: update.enable-services)
# Results: reset / restart-first / installing / disabled / no-cache (neither
# folder exists) / not-stopped (a service did not stop within $stopSeconds;
# started again, nothing renamed) / in-use (a folder could not be renamed;
# anything renamed is renamed back, services started again).
# Facts: folder (the SoftwareDistribution folder), stamp (the time added to
# the old names), renamed (the folders renamed).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$stopSeconds = 30
# In this order: Update Orchestrator first, it starts Windows Update again.
$services = @('UsoSvc', 'wuauserv', 'BITS', 'DoSvc')
$mustStop = @('wuauserv', 'BITS')
$folder = Join-Path $env:SystemRoot 'SoftwareDistribution'
$parts = @('DataStore', 'Download')

function Get-Result {
    param([string]$Code, [string]$Stamp = '', [string[]]$Renamed = @())
    return [pscustomobject]@{
        result = $Code
        facts  = [ordered]@{ folder = $folder; stamp = $Stamp; renamed = $Renamed }
    }
}

$restartKeys = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending',
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired'
)
$installer = Get-Service -Name 'TrustedInstaller' -ErrorAction SilentlyContinue
if ((@($restartKeys | Where-Object { Test-Path -LiteralPath $_ }).Count -gt 0) -or
    (($null -ne $installer) -and ([string]$installer.StartType -eq 'Automatic'))) {
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
        if ($mustStop -contains $name) {
            $allStopped = $false
            break
        }
    }
}

$code = 'not-stopped'
$stamp = ''
$renamed = New-Object System.Collections.Generic.List[string]
if ($allStopped) {
    $present = @($parts | Where-Object { Test-Path -LiteralPath (Join-Path $folder $_) })
    if ($present.Count -eq 0) {
        $code = 'no-cache'
    }
    else {
        $stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
        $code = 'reset'
        foreach ($part in $present) {
            $old = '{0}.old-{1}' -f $part, $stamp
            try {
                Rename-Item -LiteralPath (Join-Path $folder $part) -NewName $old
                $renamed.Add($old)
            }
            catch {
                Write-Verbose ('{0} could not be renamed: {1}' -f $part, $_.Exception.Message)
                $code = 'in-use'
                break
            }
        }
        if ($code -eq 'in-use') {
            # Put back what was renamed, so that nothing is half done.
            foreach ($old in $renamed) {
                $name = $old.Substring(0, $old.IndexOf('.old-'))
                try {
                    Rename-Item -LiteralPath (Join-Path $folder $old) -NewName $name
                }
                catch {
                    Write-Verbose ('{0} could not be renamed back: {1}' -f $old, $_.Exception.Message)
                }
            }
            $renamed.Clear()
            $stamp = ''
        }
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
Get-Result $code $stamp $renamed.ToArray()
