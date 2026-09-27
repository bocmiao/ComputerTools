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
# - It then waits up to 15 seconds from the stop (measured by the clock, not
#   by counting polls, since each poll takes time too), polling every 500 ms,
#   for an explorer.exe in the same session that was not running before (a new
#   PID, or the same PID with a different start time). The YAML's timeout_sec
#   leaves plenty of room above that, so the engine never kills the host while
#   Explorer is stopped.
# The stopping and waiting are a shared block (explorer-restart), identical in
# tools/system/rebuild-icon-cache.ps1.
# If Explorer does not come back, or was not running at all, the YAML tells the
# user how to start it from Task Manager, which starts it as the user.
#
# Result codes: restarted (ok), not-restarted, not-running, auto-restart-off
# (advice). Facts: stopped (processes stopped), waited_sec.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block explorer-restart: identical in tools/system/restart-explorer.ps1 and tools/system/rebuild-icon-cache.ps1 (medkit-data check compares them) ----
# Explorer is never started from here (it would run elevated, and so would
# every program started from the desktop): it is stopped, and Winlogon starts
# it again as the signed-in user (AutoRestartShell = 1, the default; a forced
# stop leaves a non-zero exit code, which counts as unexpected).
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

# False when Winlogon would not start the shell again (AutoRestartShell = 0).
# A missing value means the default (1).
function Test-ShellAutoRestart {
    $value = $null
    try {
        $value = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon' -Name 'AutoRestartShell').AutoRestartShell
    }
    catch {
        $value = $null
    }
    return (-not (($null -ne $value) -and ([string]$value -eq '0')))
}

# Stops the given explorer.exe processes. Returns what Wait-ShellRestart
# needs: how many were stopped, their PIDs and start times, and a stopwatch
# started at the first stop (the wait is measured from there).
function Stop-Shell {
    param([object[]]$Processes)
    $old = @{}
    foreach ($p in $Processes) {
        $old[[int]$p.Id] = Get-StartTick $p
    }
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $stopped = 0
    $errors = New-Object System.Collections.Generic.List[string]
    foreach ($p in $Processes) {
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
    return [pscustomobject]@{ Stopped = $stopped; Old = $old; Watch = $watch }
}

# Waits, up to 15 seconds from the stop and polling every 500 ms, for an
# explorer.exe in the session that was not running before (a new PID, or the
# same PID with a different start time). True when one came.
function Wait-ShellRestart {
    param([int]$SessionId, $Stop)
    $pollMs = 500
    $maxWaitMs = 15000
    $restarted = $false
    while ((-not $restarted) -and ($Stop.Watch.ElapsedMilliseconds -lt $maxWaitMs)) {
        $sleepMs = [int][math]::Min($pollMs, [math]::Max(1, $maxWaitMs - $Stop.Watch.ElapsedMilliseconds))
        Start-Sleep -Milliseconds $sleepMs
        foreach ($p in @(Get-ShellProcess -SessionId $SessionId)) {
            $id = [int]$p.Id
            if ((-not $Stop.Old.ContainsKey($id)) -or ($Stop.Old[$id] -ne (Get-StartTick $p))) {
                $restarted = $true
            }
        }
    }
    $Stop.Watch.Stop()
    return $restarted
}
# ---- end of shared block explorer-restart ----

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
if (-not (Test-ShellAutoRestart)) {
    New-Result -Code 'auto-restart-off' -Stopped 0 -Waited 0
    return
}

$stop = Stop-Shell -Processes $before
$restarted = Wait-ShellRestart -SessionId $session -Stop $stop

$code = 'not-restarted'
if ($restarted) {
    $code = 'restarted'
}
New-Result -Code $code -Stopped $stop.Stopped -Waited $stop.Watch.Elapsed.TotalSeconds
