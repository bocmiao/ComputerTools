# Feature: system.enable-ctf-monitor -- run (and prepare)
# Enables the scheduled task MsCtfMonitor again (Windows has it enabled), then
# runs it once so that the input method comes back without signing out.
# -Prepare: returns before = { state } (Ready, Running or Disabled).
# Run: -Before is that JSON. Returns skipped (nothing changed) when the state
#   is no longer what was recorded or the task is missing. Otherwise enables
#   the task and reads it back: it must no longer be Disabled, or the script
#   throws (the engine then runs the undo script). Running the task is best
#   effort: the change is the enabled task.

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = ''
)

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
if ($Prepare) {
    return [pscustomobject]@{ before = [ordered]@{ state = $state } }
}

$recorded = ConvertFrom-Json -InputObject $Before
if (($state -eq 'missing') -or ([string]$recorded.state -ne $state)) {
    return [pscustomobject]@{ skipped = $true }
}
$null = Enable-ScheduledTask -TaskPath $ctfTaskPath -TaskName $ctfTaskName
$after = Get-CtfTaskState
if ($after -eq 'Disabled') {
    throw 'The MsCtfMonitor task is still disabled'
}
try {
    Start-ScheduledTask -TaskPath $ctfTaskPath -TaskName $ctfTaskName
}
catch {
    Write-Verbose ('The MsCtfMonitor task could not be run now: {0}' -f $_.Exception.Message)
}
[pscustomobject]@{ after = [ordered]@{ state = $after } }
