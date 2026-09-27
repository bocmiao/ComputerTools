# Feature: system.enable-ctf-monitor -- undo
# Disables the scheduled task MsCtfMonitor again when it was disabled before
# the run script (-Before { state }). The running ctfmon.exe is left alone.

[CmdletBinding()]
param(
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

$recorded = ConvertFrom-Json -InputObject $Before
if (([string]$recorded.state -eq 'Disabled') -and ((Get-CtfTaskState) -ne 'missing')) {
    $null = Disable-ScheduledTask -TaskPath $ctfTaskPath -TaskName $ctfTaskName
}
[pscustomobject]@{ result = 'ok' }
