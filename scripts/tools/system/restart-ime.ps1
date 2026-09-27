# Tool: system.restart-ime (action)
# Restarts the input method host for the signed-in user the way Windows starts
# it at sign-in: ends ctfmon.exe in this session, then runs the scheduled task
# MsCtfMonitor (it runs as the signed-in user, so ctfmon does not run
# elevated), and waits up to 5 seconds for ctfmon.exe to come back.
# Results: started / not-started (the task ran, ctfmon did not appear) /
# disabled (the task is disabled; nothing was ended) / missing (no task;
# nothing was ended).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block ctf-task: identical in checks/system/input-method.ps1, tools/system/restart-ime.ps1 and features/system/ctf-monitor-*.ps1 (medkit-data check compares them) ----
# The scheduled task that starts the input method host (ctfmon.exe) for the
# signed-in user: \Microsoft\Windows\TextServicesFramework\MsCtfMonitor. "Optimizer"
# tools sometimes disable it; then the input method bar and Chinese input are
# gone after the next sign-in. State: Ready, Running, Disabled, or "missing".
$ctfTaskPath = '\Microsoft\Windows\TextServicesFramework\'
$ctfTaskName = 'MsCtfMonitor'

function Get-CtfTaskState {
    $task = Get-ScheduledTask -TaskPath $ctfTaskPath -TaskName $ctfTaskName -ErrorAction SilentlyContinue
    if ($null -eq $task) {
        return 'missing'
    }
    return [string]$task.State
}
# ---- end of shared block ctf-task ----

# ---- shared block ctf-process: identical in checks/system/input-method.ps1 and tools/system/restart-ime.ps1 (medkit-data check compares them) ----
# ctfmon.exe running in this session. medkit runs in the session of the
# signed-in user (elevation keeps the session), and so does this script.
function Test-CtfRunning {
    $session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
    $found = @(Get-Process -Name 'ctfmon' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $session })
    return $found.Count -gt 0
}
# ---- end of shared block ctf-process ----

$state = Get-CtfTaskState
if ($state -eq 'missing') {
    [pscustomobject]@{ result = 'missing'; facts = [ordered]@{} }
    return
}
if ($state -eq 'Disabled') {
    [pscustomobject]@{ result = 'disabled'; facts = [ordered]@{} }
    return
}

$session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
foreach ($process in @(Get-Process -Name 'ctfmon' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $session })) {
    try {
        Stop-Process -Id $process.Id -Force -ErrorAction Stop
    }
    catch {
        Write-Verbose ('ctfmon.exe could not be ended: {0}' -f $_.Exception.Message)
    }
}
Start-ScheduledTask -TaskPath $ctfTaskPath -TaskName $ctfTaskName

$watch = [System.Diagnostics.Stopwatch]::StartNew()
$running = $false
while ((-not $running) -and ($watch.ElapsedMilliseconds -lt 5000)) {
    Start-Sleep -Milliseconds 250
    $running = Test-CtfRunning
}
$code = 'not-started'
if ($running) {
    $code = 'started'
}
[pscustomobject]@{ result = $code; facts = [ordered]@{} }
