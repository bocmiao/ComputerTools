# Tool: audio.restart-service (action)
# Starts, or restarts, Windows Audio Endpoint Builder and Windows Audio
# without changing their startup type. Restarting the Endpoint Builder stops
# Windows Audio too (it depends on it), so Windows Audio is started again
# afterwards. Waits up to 20 seconds for both to run.
# Results: restarted / started / disabled (nothing done) / missing / failed.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$names = @('AudioEndpointBuilder', 'Audiosrv')
$services = @($names | ForEach-Object { Get-Service -Name $_ -ErrorAction SilentlyContinue } | Where-Object { $null -ne $_ })
if ($services.Count -ne $names.Count) {
    [pscustomobject]@{ result = 'missing'; facts = [ordered]@{} }
    return
}
if (@($services | Where-Object { [string]$_.StartType -eq 'Disabled' }).Count -gt 0) {
    [pscustomobject]@{ result = 'disabled'; facts = [ordered]@{} }
    return
}

$wasRunning = (Get-Service -Name 'Audiosrv').Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running
try {
    if ($wasRunning) {
        Restart-Service -Name 'AudioEndpointBuilder' -Force -ErrorAction Stop
    }
    foreach ($name in $names) {
        $service = Get-Service -Name $name
        if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Running) {
            Start-Service -Name $name -ErrorAction Stop
        }
        $service = Get-Service -Name $name
        $service.WaitForStatus([System.ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(20))
    }
    $code = 'started'
    if ($wasRunning) {
        $code = 'restarted'
    }
}
catch {
    # A failed restart can leave Windows Audio stopped. Try to bring it back.
    foreach ($name in $names) {
        try {
            if ((Get-Service -Name $name).Status -ne [System.ServiceProcess.ServiceControllerStatus]::Running) {
                Start-Service -Name $name -ErrorAction Stop
            }
        }
        catch {
            Write-Verbose ('{0} could not be started again: {1}' -f $name, $_.Exception.Message)
        }
    }
    $code = 'failed'
}
[pscustomobject]@{ result = $code; facts = [ordered]@{} }
