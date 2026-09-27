# Feature: system.enable-ctf-monitor -- detect
# Is the scheduled task MsCtfMonitor enabled? States:
#   applied      enabled (Ready or Running)
#   not-applied  disabled
#   unknown      the task is missing
# Read-only.

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

$state = Get-CtfTaskState
$code = 'applied'
if ($state -eq 'Disabled') {
    $code = 'not-applied'
}
elseif ($state -eq 'missing') {
    $code = 'unknown'
}
[pscustomobject]@{ state = $code }
