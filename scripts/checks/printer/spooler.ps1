# Check whether the local Print Spooler can accept print jobs.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$service = Get-Service -Name 'Spooler' -ErrorAction SilentlyContinue
$code = 'missing'
if ($null -ne $service) {
    if ($service.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running) {
        $code = 'running'
    }
    else {
        $start = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\Spooler' -Name 'Start' -ErrorAction Stop).Start
        if ([int]$start -eq 4) { $code = 'disabled' }
        else { $code = 'stopped' }
    }
}
[pscustomobject]@{ result = $code; facts = [ordered]@{} }
