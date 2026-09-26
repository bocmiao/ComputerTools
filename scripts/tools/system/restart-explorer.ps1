# Tool: system.restart-explorer
# Restarts Windows Explorer (the desktop, taskbar and Start menu) when they are
# stuck, or after a setting that only takes effect in a new Explorer.
#
# Why it works this way:
# - This script runs as administrator. It must never start explorer.exe
#   itself: an Explorer started from an elevated process runs elevated, and when
#   it becomes the shell the whole desktop, and every program started from it,
#   would run as administrator.
# - Instead it relies on Winlogon: with AutoRestartShell = 1 (the default,
#   HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon), Winlogon starts
#   the shell again, as the signed-in user, when it ends unexpectedly. A forced
#   stop (Stop-Process -Force, TerminateProcess) leaves a non-zero exit code,
#   which counts as unexpected. When AutoRestartShell is 0 nothing is stopped
#   (auto-restart-off): the desktop would not come back by itself.
# - Only explorer.exe processes in this script's own session are stopped
#   ((Get-Process -Id $PID).SessionId), never another signed-in user's.
# - It then waits up to 15 seconds, polling every 500 ms, for an explorer.exe
#   in the same session that was not running before (a new PID, or the same PID
#   with a different start time).
# If Explorer does not come back, or was not running at all, the YAML tells the
# user how to start it from Task Manager, which starts it as the user.
#
# Result codes: restarted (ok), not-restarted, not-running, auto-restart-off
# (advice). Facts: stopped (processes stopped), waited_sec.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$pollMs = 500
$maxPolls = 30

function Get-ShellProcess {
    param([int]$SessionId)
    return @(Get-Process -Name 'explorer' -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $SessionId })
}

# Start time as ticks, or 0 when it cannot be read.
function Get-StartTick {
    param($Process)
    try {
        return [long]$Process.StartTime.ToUniversalTime().Ticks
    }
    catch {
        return [long]0
    }
}

function New-Result {
    param([string]$Code, [int]$Stopped, [double]$Waited)
    return [pscustomobject]@{
        result = $Code
        facts  = [ordered]@{
            stopped    = $Stopped
            waited_sec = [math]::Round($Waited, 1)
        }
    }
}

$session = (Get-Process -Id $PID).SessionId

$before = @(Get-ShellProcess -SessionId $session)
if ($before.Count -eq 0) {
    New-Result -Code 'not-running' -Stopped 0 -Waited 0
    return
}

$autoRestart = $null
try {
    $autoRestart = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon' -Name 'AutoRestartShell').AutoRestartShell
}
catch {
    # Missing value: the default (1) applies.
    $autoRestart = $null
}
if (($null -ne $autoRestart) -and ([string]$autoRestart -eq '0')) {
    New-Result -Code 'auto-restart-off' -Stopped 0 -Waited 0
    return
}

# PID -> start time of the processes that are about to be stopped.
$old = @{}
foreach ($p in $before) {
    $old[[int]$p.Id] = Get-StartTick $p
}

$stopped = 0
$errors = New-Object System.Collections.Generic.List[string]
foreach ($p in $before) {
    try {
        Stop-Process -Id $p.Id -Force
        $stopped++
    }
    catch {
        # A process that ended on its own in the meantime is fine.
        $still = @(Get-Process -Id $p.Id -ErrorAction SilentlyContinue)
        if ($still.Count -eq 0) {
            $stopped++
        }
        else {
            $errors.Add($_.Exception.Message)
        }
    }
}
if ($stopped -eq 0) {
    throw ('Could not stop explorer.exe: {0}' -f ($errors -join '; '))
}

$watch = [System.Diagnostics.Stopwatch]::StartNew()
$restarted = $false
for ($i = 0; ($i -lt $maxPolls) -and (-not $restarted); $i++) {
    Start-Sleep -Milliseconds $pollMs
    foreach ($p in @(Get-ShellProcess -SessionId $session)) {
        $id = [int]$p.Id
        if ((-not $old.ContainsKey($id)) -or ($old[$id] -ne (Get-StartTick $p))) {
            $restarted = $true
        }
    }
}
$watch.Stop()

$code = 'not-restarted'
if ($restarted) {
    $code = 'restarted'
}
New-Result -Code $code -Stopped $stopped -Waited $watch.Elapsed.TotalSeconds
