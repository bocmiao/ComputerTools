# Clear the print queue: stop Print Spooler, delete the spooled jobs (the .SHD
# and .SPL files in the spool folder: one pair per job), then start it again
# together with the services that depend on it and were running (Fax).
# Microsoft's steps for jobs that are stuck in the queue do the same.
# Only these two kinds of files, only directly in the spool folder, and only a
# spool folder inside the Windows folder are touched; the documents the jobs
# were printed from are not. Nothing is queued: nothing is stopped.
# The spool folder is DefaultSpoolDirectory under
# HKLM\SYSTEM\CurrentControlSet\Control\Print\Printers, by default
# %SystemRoot%\System32\spool\PRINTERS.
# Result codes: cleared (facts: jobs) / partial (facts: jobs, left: files that
# could not be deleted) / empty / disabled / missing / failed (the spooler did
# not stop or did not start again).

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

$windows = ([string][Environment]::GetFolderPath('Windows')).TrimEnd('\')
$folder = $windows + '\System32\spool\PRINTERS'
try {
    $configured = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Print\Printers' -Name 'DefaultSpoolDirectory' -ErrorAction Stop).DefaultSpoolDirectory
    if ($configured -is [string]) {
        $full = [IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($configured.Trim())).TrimEnd('\')
        if ($full.StartsWith($windows + '\', [StringComparison]::OrdinalIgnoreCase)) {
            $folder = $full
        }
    }
}
catch {
    Write-Verbose ('Using the default spool folder: {0}' -f $_.Exception.Message)
}

function Get-SpoolFile {
    if (-not (Test-Path -LiteralPath $folder -PathType Container)) {
        return @()
    }
    return @(Get-ChildItem -LiteralPath $folder -File -Force -ErrorAction SilentlyContinue |
            Where-Object { @('.shd', '.spl') -contains $_.Extension.ToLowerInvariant() })
}

$files = @(Get-SpoolFile)
if ($files.Count -eq 0) {
    [pscustomobject]@{ result = 'empty'; facts = [ordered]@{} }
    return
}
# One .SHD (the job's settings) and one .SPL (its data) per job
$shadow = @($files | Where-Object { $_.Extension -ieq '.shd' }).Count
$jobs = [Math]::Max($shadow, $files.Count - $shadow)

$running = [System.ServiceProcess.ServiceControllerStatus]::Running
$stopped = [System.ServiceProcess.ServiceControllerStatus]::Stopped
$dependents = @($service.DependentServices | Where-Object { $_.Status -eq $running } | ForEach-Object { $_.Name })
try {
    Stop-Service -Name 'Spooler' -Force -ErrorAction Stop
    $service = Get-Service -Name 'Spooler' -ErrorAction Stop
    $service.WaitForStatus($stopped, [TimeSpan]::FromSeconds(20))
}
catch {
    Write-Verbose ('The print spooler did not stop: {0}' -f $_.Exception.Message)
    try {
        Start-Service -Name 'Spooler' -ErrorAction Stop
    }
    catch {
        Write-Verbose ('The print spooler could not be started again: {0}' -f $_.Exception.Message)
    }
    [pscustomobject]@{ result = 'failed'; facts = [ordered]@{} }
    return
}

$left = 0
foreach ($file in $files) {
    try {
        Remove-Item -LiteralPath $file.FullName -Force -ErrorAction Stop
    }
    catch {
        $left++
    }
}

$code = 'cleared'
if ($left -gt 0) {
    $code = 'partial'
}
try {
    Start-Service -Name 'Spooler' -ErrorAction Stop
    $service = Get-Service -Name 'Spooler' -ErrorAction Stop
    $service.WaitForStatus($running, [TimeSpan]::FromSeconds(20))
    foreach ($name in $dependents) {
        try {
            Start-Service -Name $name -ErrorAction Stop
        }
        catch {
            Write-Verbose ('{0} could not be started again: {1}' -f $name, $_.Exception.Message)
        }
    }
}
catch {
    $code = 'failed'
}

[pscustomobject]@{
    result = $code
    facts  = [ordered]@{ jobs = $jobs; left = $left }
}
