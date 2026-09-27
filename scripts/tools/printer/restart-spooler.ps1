# Start or restart Print Spooler without changing its configured startup type.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$service = Get-Service -Name 'Spooler' -ErrorAction SilentlyContinue
if ($null -eq $service) {
    [pscustomobject]@{ result = 'missing'; facts = [ordered]@{} }
    return
}
$start = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\Spooler' -Name 'Start' -ErrorAction Stop).Start
if ([int]$start -eq 4) {
    [pscustomobject]@{ result = 'disabled'; facts = [ordered]@{} }
    return
}

$wasRunning = $service.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running
try {
    if ($wasRunning) { Restart-Service -Name 'Spooler' -ErrorAction Stop }
    else { Start-Service -Name 'Spooler' -ErrorAction Stop }
    $service = Get-Service -Name 'Spooler' -ErrorAction Stop
    $service.WaitForStatus([System.ServiceProcess.ServiceControllerStatus]::Running, [TimeSpan]::FromSeconds(20))
    $code = if ($wasRunning) { 'restarted' } else { 'started' }
}
catch {
    # A failed restart can leave the service stopped. Try to bring it back.
    try {
        $service = Get-Service -Name 'Spooler' -ErrorAction Stop
        if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Running) {
            Start-Service -Name 'Spooler' -ErrorAction Stop
        }
    }
    catch {
        Write-Verbose ('The print spooler could not be started again: {0}' -f $_.Exception.Message)
    }
    $code = 'failed'
}
[pscustomobject]@{ result = $code; facts = [ordered]@{} }
