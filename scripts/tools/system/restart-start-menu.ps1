# Tool: system.restart-start-menu (action)
# Restarts the Start menu, search and the notification centre when they are
# stuck or do not open, without restarting Explorer (the taskbar and the open
# folder windows stay). The processes that show them are stopped, and Windows
# starts them again, as the user, when they are needed (each signed-in user
# has their own StartMenuExperienceHost in their session: Microsoft,
# "Troubleshoot Start menu errors"):
#   StartMenuExperienceHost   the Start menu (Windows 10 1903 and later)
#   SearchHost                search (Windows 11)
#   SearchApp                 search (Windows 10 2004 and later)
#   SearchUI                  search and Cortana (earlier Windows 10)
#   ShellExperienceHost       the notification centre, quick settings, the
#                             calendar, the network and volume panels (and
#                             the Start menu on Windows 10 1809 and earlier)
# Nothing is started from here (it would run elevated). Only processes in this
# script's own session ((Get-Process -Id $PID).SessionId) are stopped, never
# another signed-in user's. Explorer starts them again: when it is not running
# in the session nothing is stopped (no-explorer). After stopping, it waits up
# to $maxWaitMs for a new StartMenuExperienceHost, when one was stopped, to
# tell whether Start is already back.
# Result codes: restarted / stopped (Start comes back when it is opened) /
# none (none of them was running) / no-explorer.
# Facts: stopped (processes stopped), waited_sec.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$names = @('StartMenuExperienceHost', 'SearchHost', 'SearchApp', 'SearchUI', 'ShellExperienceHost')
$maxWaitMs = 10000
$pollMs = 500

function Get-SessionProcess {
    param([string[]]$Name, [int]$SessionId)
    return @(Get-Process -Name $Name -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $SessionId })
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
if ((Get-SessionProcess -Name @('explorer') -SessionId $session).Count -eq 0) {
    New-Result -Code 'no-explorer' -Stopped 0 -Waited 0
    return
}
$targets = Get-SessionProcess -Name $names -SessionId $session
if ($targets.Count -eq 0) {
    New-Result -Code 'none' -Stopped 0 -Waited 0
    return
}

$oldStart = @{}
foreach ($p in $targets) {
    if ($p.ProcessName -eq 'StartMenuExperienceHost') {
        $oldStart[[int]$p.Id] = Get-StartTick $p
    }
}

$watch = [System.Diagnostics.Stopwatch]::StartNew()
$stopped = 0
$errors = New-Object System.Collections.Generic.List[string]
foreach ($p in $targets) {
    try {
        Stop-Process -Id $p.Id -Force
        $stopped++
    }
    catch {
        # A process that ended on its own in the meantime is fine.
        if (@(Get-Process -Id $p.Id -ErrorAction SilentlyContinue).Count -eq 0) {
            $stopped++
        }
        else {
            $errors.Add($_.Exception.Message)
        }
    }
}
if ($stopped -eq 0) {
    throw ('Could not stop the Start menu: {0}' -f ($errors -join '; '))
}

# Waits, polling every $pollMs, for a StartMenuExperienceHost that was not
# running before (a new PID, or the same PID with a different start time).
$back = $false
if ($oldStart.Count -gt 0) {
    while ((-not $back) -and ($watch.ElapsedMilliseconds -lt $maxWaitMs)) {
        Start-Sleep -Milliseconds ([int][math]::Min($pollMs, [math]::Max(1, $maxWaitMs - $watch.ElapsedMilliseconds)))
        foreach ($p in (Get-SessionProcess -Name @('StartMenuExperienceHost') -SessionId $session)) {
            $id = [int]$p.Id
            if ((-not $oldStart.ContainsKey($id)) -or ($oldStart[$id] -ne (Get-StartTick $p))) {
                $back = $true
            }
        }
    }
}
$watch.Stop()

$code = 'stopped'
if ($back) {
    $code = 'restarted'
}
New-Result -Code $code -Stopped $stopped -Waited $watch.Elapsed.TotalSeconds
