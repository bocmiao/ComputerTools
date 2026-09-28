# Check: system.taskbar-flashing
# Does the taskbar flash apps that want attention (the logged-in user's
# "Show flashing on taskbar apps", Windows 11 22H2 and later)? While a button
# flashes, the taskbar stays up, even over full-screen videos and games.
# The setting is the DWORD TaskbarFlashing under Explorer\Advanced: 0 = off,
# 1 or missing = on (Windows' default), as in Shawn Brink's registry files for
# ElevenForum (slpcat/NT6xTweaking, MIT). Read-only.
# Result codes: old-windows (before Windows 11 22H2, no such setting) /
# off / on.
# Facts: value (the DWORD, -1 when missing).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

# Windows' build number, or 0 when it cannot be read.
function Get-Build {
    $key = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    foreach ($name in @('CurrentBuild', 'CurrentBuildNumber')) {
        try {
            $text = [string](Get-ItemProperty -LiteralPath $key -Name $name -ErrorAction Stop).$name
        }
        catch {
            continue
        }
        $number = 0
        if ([int]::TryParse($text, [ref]$number)) {
            return $number
        }
    }
    return 0
}

$build = Get-Build
if (($build -gt 0) -and ($build -lt 22621)) {
    return [pscustomobject]@{ result = 'old-windows'; facts = [ordered]@{ value = -1 } }
}

$value = -1
try {
    $item = Get-ItemProperty -LiteralPath ($UserHive.TrimEnd('\') + '\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced') -Name 'TaskbarFlashing' -ErrorAction Stop
    if ($item.TaskbarFlashing -is [int]) {
        $value = $item.TaskbarFlashing
    }
}
catch {
    $value = -1
}

[pscustomobject]@{
    result = $(if ($value -eq 0) { 'off' } else { 'on' })
    facts  = [ordered]@{ value = $value }
}
