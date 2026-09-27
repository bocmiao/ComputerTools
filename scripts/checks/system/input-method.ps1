# Check: system.input-method
# Why the input method (Chinese input) may be gone, in this order:
#   task-disabled  the scheduled task MsCtfMonitor is disabled (fix:
#                  system.enable-ctf-monitor)
#   task-missing   the task is gone and ctfmon.exe is not running
#   not-running    ctfmon.exe is not running in this session (tool:
#                  system.restart-ime)
#   no-chinese     no Chinese language in the signed-in user's language list
#                  (<user hive>\Control Panel\International\User Profile,
#                  Languages); an unreadable list counts as having one
#   ok
# Read-only. Outputs one object: { result, facts }. Facts: task_state,
# running, chinese.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
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

# ---- shared block ctf-process: identical in checks/system/input-method.ps1 and tools/system/restart-ime.ps1 (medkit-data check compares them) ----
# ctfmon.exe running in this session. medkit runs in the session of the
# signed-in user (elevation keeps the session), and so does this script.
function Test-CtfRunning {
    $session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
    $found = @(Get-Process -Name 'ctfmon' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $session })
    return $found.Count -gt 0
}
# ---- end of shared block ctf-process ----

# The language tags in the signed-in user's list; $null when it cannot be read.
function Get-UserLanguage {
    param([string]$UserHive)
    $key = $UserHive.TrimEnd('\') + '\Control Panel\International\User Profile'
    if (-not (Test-Path -LiteralPath $key)) {
        return $null
    }
    $value = (Get-Item -LiteralPath $key).GetValue('Languages', $null)
    if ($null -eq $value) {
        return $null
    }
    return , @($value | ForEach-Object { ([string]$_).Trim() } | Where-Object { $_.Length -gt 0 })
}

$taskState = Get-CtfTaskState
$running = Test-CtfRunning
$languages = Get-UserLanguage -UserHive $UserHive
$chinese = ($null -eq $languages) -or (@($languages | Where-Object { $_ -like 'zh-*' }).Count -gt 0)

$result = 'ok'
if ($taskState -eq 'Disabled') {
    $result = 'task-disabled'
}
elseif (-not $running) {
    if ($taskState -eq 'missing') {
        $result = 'task-missing'
    }
    else {
        $result = 'not-running'
    }
}
elseif (-not $chinese) {
    $result = 'no-chinese'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        task_state = $taskState
        running    = $running
        chinese    = $chinese
    }
}
