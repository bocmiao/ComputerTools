# Tool: system.rebuild-icon-cache (action)
# Rebuilds the icon and thumbnail caches of the signed-in user, for icons on
# the desktop or in folders that turned into blank pages or show the wrong
# picture, and thumbnails of pictures and videos that are wrong or missing.
# The caches are files Explorer keeps in the user's local application data
# folder (%LOCALAPPDATA%):
#   Microsoft\Windows\Explorer\iconcache_*.db   icons (Windows 8 and later)
#   Microsoft\Windows\Explorer\thumbcache_*.db  thumbnails
#   IconCache.db                                icons (Windows 7; still left on
#                                               some PCs)
# Explorer holds them open, so it is stopped first, the files are deleted, and
# Winlogon starts Explorer again as the user, which builds new caches as it
# shows the icons again. Stopping and waiting are the same as in
# restart-explorer.ps1 (shared block explorer-restart): nothing is stopped when
# AutoRestartShell is 0 (auto-restart-off) or Explorer is not running
# (not-running). The files are deleted right after the stopped processes have
# exited, before Winlogon has started a new Explorer; a file another program
# still holds open is left alone and counted (locked).
# The folder: "Local AppData" under Shell Folders in the user's hive
# (-UserHive; Windows keeps the absolute path there), else the profile folder
# from ProfileList plus AppData\Local. Only files with the names above,
# directly in those two folders, are deleted; folders that are links
# (junctions, symbolic links) are not followed.
# Nothing is done either when an icon override under HKLM\...\Explorer\Shell
# Icons names a file that does not exist (shell-icons-broken): with such an
# entry Explorer may not build the icon cache again and every icon is gone
# until it is fixed (seen after "remove the shortcut arrow" tweaks, Microsoft
# Q&A). An entry is "path,index" or just a path; environment variables are
# expanded, quotes trimmed; an empty path, one starting with "-", or one that
# is not a file counts as broken.
# Privacy: no path is output (it contains the user name), only counts.
# Results: rebuilt, partly (some files were held open), nothing (no cache
# files), no-folder, shell-icons-broken, not-restarted, not-running,
# auto-restart-off.
# Facts: deleted, locked, freed ("85 MB", "320 KB"), stopped, waited_sec,
# broken_icons.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

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

# "85 MB", "320 KB" (at least 1 KB when anything was freed).
function Format-Size {
    param([long]$Bytes)
    $invariant = [Globalization.CultureInfo]::InvariantCulture
    if ($Bytes -ge 1MB) {
        return ($Bytes / 1MB).ToString('0.#', $invariant) + ' MB'
    }
    return ([math]::Max([math]::Ceiling($Bytes / 1KB), [long]($Bytes -gt 0))).ToString('0', $invariant) + ' KB'
}

function New-Result {
    param([string]$Code, [int]$Deleted, [int]$Locked, [long]$Freed, [int]$Stopped, [double]$Waited, [int]$Broken)
    return [pscustomobject]@{
        result = $Code
        facts  = [ordered]@{
            deleted      = $Deleted
            locked       = $Locked
            freed        = (Format-Size $Freed)
            stopped      = $Stopped
            waited_sec   = [math]::Round($Waited, 1)
            broken_icons = $Broken
        }
    }
}

# How many icon overrides (Shell Icons) name a file that is not there.
function Get-BrokenShellIconCount {
    $key = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Shell Icons'
    $values = $null
    try {
        $values = Get-ItemProperty -LiteralPath $key -ErrorAction Stop
    }
    catch {
        return 0
    }
    $broken = 0
    foreach ($property in @($values.PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' })) {
        $text = [Environment]::ExpandEnvironmentVariables(([string]$property.Value).Trim())
        $m = [regex]::Match($text, '^(.*),\s*-?\d+$')
        if ($m.Success) {
            $text = $m.Groups[1].Value
        }
        $path = $text.Trim().Trim('"').Trim()
        if (($path.Length -eq 0) -or $path.StartsWith('-') -or (-not (Test-Path -LiteralPath $path -PathType Leaf))) {
            $broken++
        }
    }
    return $broken
}

function Test-LinkFolder {
    param([string]$Path)
    try {
        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    }
    catch {
        return $true
    }
    return ((-not $item.PSIsContainer) -or (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0))
}

# The signed-in user's local application data folder, or ''.
function Get-LocalAppData {
    param([string]$Hive)
    $shellFolders = $Hive.TrimEnd('\') + '\Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
    $path = ''
    try {
        $path = [string](Get-ItemProperty -LiteralPath $shellFolders -Name 'Local AppData' -ErrorAction Stop).'Local AppData'
    }
    catch {
        $path = ''
    }
    if (($path.Length -gt 0) -and ($path -notmatch '%') -and [IO.Path]::IsPathRooted($path)) {
        return $path
    }
    if ($Hive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
        $profileKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $Matches[1]
        try {
            $profilePath = [string](Get-ItemProperty -LiteralPath $profileKey -Name 'ProfileImagePath' -ErrorAction Stop).ProfileImagePath
        }
        catch {
            $profilePath = ''
        }
        if ($profilePath.Length -gt 0) {
            # ProfileImagePath is like %SystemDrive%\Users\name: only machine-wide variables
            return (Join-Path ([Environment]::ExpandEnvironmentVariables($profilePath)) 'AppData\Local')
        }
        return ''
    }
    return [string]$env:LOCALAPPDATA
}

# The cache files there now (only regular files with the exact names).
function Get-CacheFile {
    param([string]$LocalAppData)
    $files = New-Object System.Collections.Generic.List[object]
    $explorer = Join-Path $LocalAppData 'Microsoft\Windows\Explorer'
    $places = @(
        @{ Folder = $explorer; Pattern = 'iconcache_*.db' },
        @{ Folder = $explorer; Pattern = 'thumbcache_*.db' },
        @{ Folder = $LocalAppData; Pattern = 'IconCache.db' }
    )
    foreach ($place in $places) {
        if (Test-LinkFolder $place.Folder) {
            continue
        }
        foreach ($file in @(Get-ChildItem -LiteralPath $place.Folder -Filter $place.Pattern -File -Force -ErrorAction SilentlyContinue)) {
            if (($file.Name -like $place.Pattern) -and (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0)) {
                $files.Add($file)
            }
        }
    }
    return $files.ToArray()
}

$session = (Get-Process -Id $PID).SessionId

$before = @(Get-ShellProcess -SessionId $session)
if ($before.Count -eq 0) {
    New-Result -Code 'not-running' -Deleted 0 -Locked 0 -Freed 0 -Stopped 0 -Waited 0 -Broken 0
    return
}
$localAppData = Get-LocalAppData $UserHive
if (($localAppData.Length -eq 0) -or (Test-LinkFolder $localAppData)) {
    New-Result -Code 'no-folder' -Deleted 0 -Locked 0 -Freed 0 -Stopped 0 -Waited 0 -Broken 0
    return
}
if (@(Get-CacheFile $localAppData).Count -eq 0) {
    New-Result -Code 'nothing' -Deleted 0 -Locked 0 -Freed 0 -Stopped 0 -Waited 0 -Broken 0
    return
}
$broken = Get-BrokenShellIconCount
if ($broken -gt 0) {
    New-Result -Code 'shell-icons-broken' -Deleted 0 -Locked 0 -Freed 0 -Stopped 0 -Waited 0 -Broken $broken
    return
}
if (-not (Test-ShellAutoRestart)) {
    New-Result -Code 'auto-restart-off' -Deleted 0 -Locked 0 -Freed 0 -Stopped 0 -Waited 0 -Broken 0
    return
}

$stop = Stop-Shell -Processes $before
# The files stay open until the processes have really exited.
foreach ($p in $before) {
    try {
        $null = $p.WaitForExit(5000)
    }
    catch {
        # Already gone.
        $null = $p
    }
}
$deleted = 0
$locked = 0
$freed = [long]0
foreach ($file in @(Get-CacheFile $localAppData)) {
    $size = [long]$file.Length
    try {
        Remove-Item -LiteralPath $file.FullName -Force -ErrorAction Stop
        $deleted++
        $freed += $size
    }
    catch {
        $locked++
    }
}
$restarted = Wait-ShellRestart -SessionId $session -Stop $stop

$code = 'rebuilt'
if (-not $restarted) {
    $code = 'not-restarted'
}
elseif ($locked -gt 0) {
    $code = 'partly'
}
New-Result -Code $code -Deleted $deleted -Locked $locked -Freed $freed -Stopped $stop.Stopped -Waited $stop.Watch.Elapsed.TotalSeconds -Broken 0
