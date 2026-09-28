# Check: security.ifeo-hijack
# Image File Execution Options "Debugger": when
# HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution
# Options\<name>.exe has a Debugger value, Windows starts that program instead
# of <name>.exe (with the original command line as its arguments). Debuggers
# use it. Malware uses it to block Task Manager, the registry editor and
# antivirus programs, and to leave a backdoor on the sign-in screen: Sticky
# Keys (sethc.exe), Ease of Access (utilman.exe) and the other accessibility
# tools can be started there before anyone signs in, with SYSTEM rights
# (MITRE ATT&CK T1546.008 and T1546.012). Process Explorer, System Informer
# and Process Hacker set it on purpose for taskmgr.exe ("Replace Task
# Manager").
# Both registry views are read (the WOW6432Node copy applies when a 32-bit
# program starts the program). Only the programs in the lists below are looked
# at. Read-only: IFEO is only reported (plan, section 5).
# Result codes, the first that applies:
#   logon-backdoor    an accessibility tool of the sign-in screen is redirected
#   hijacked          a system tool or a security program is redirected (Task
#                     Manager too, unless to one of $replacers)
#   taskmgr-missing   Task Manager is replaced by one of $replacers that is not
#                     there any more, so Task Manager does not open
#   taskmgr-replaced  Task Manager is replaced by one of $replacers
#   ok
# Facts: programs (the redirected programs of that result, file names in
#   lower case), debugger (the file name of the program started instead, for
#   the first of them). Paths are never output.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$views = @(
    'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Image File Execution Options'
)
# Accessibility tools that can be started on the sign-in screen.
$logonTools = @('sethc.exe', 'utilman.exe', 'osk.exe', 'magnify.exe', 'narrator.exe', 'displayswitch.exe', 'atbroker.exe')
# System tools people need to find and fix problems.
$systemTools = @(
    'taskmgr.exe', 'regedit.exe', 'cmd.exe', 'powershell.exe', 'mmc.exe', 'msconfig.exe', 'control.exe',
    'explorer.exe', 'rstrui.exe', 'systemsettings.exe', 'resmon.exe', 'perfmon.exe'
)
# Microsoft Defender, Windows Security, the Malicious Software Removal Tool, and
# the antivirus programs most used in China (360, Huorong, Tencent PC Manager,
# Kingsoft).
$securityTools = @(
    'msmpeng.exe', 'mpcmdrun.exe', 'securityhealthsystray.exe', 'securityhealthservice.exe', 'mrt.exe',
    '360tray.exe', '360safe.exe', '360sd.exe', 'zhudongfangyu.exe',
    'hipstray.exe', 'hipsdaemon.exe', 'usysdiag.exe',
    'qqpctray.exe', 'qqpcrtp.exe', 'qqpcmgr.exe',
    'kxetray.exe', 'kxescore.exe'
)
# Programs that replace Task Manager on purpose.
$replacers = @('procexp.exe', 'procexp64.exe', 'procexp64a.exe', 'systeminformer.exe', 'processhacker.exe')

# The Debugger value of a program in one registry view, or ''.
function Get-Debugger {
    param([string]$View, [string]$Program)
    try {
        $item = Get-ItemProperty -LiteralPath ($View + '\' + $Program) -Name 'Debugger' -ErrorAction Stop
    }
    catch {
        return ''
    }
    return ([string]$item.Debugger).Trim()
}

# The program a Debugger value starts: the quoted path, else the text up to
# ".exe", else up to the first space.
function Get-CommandPath {
    param([string]$Command)
    $t = $Command.Trim()
    if ($t.StartsWith('"')) {
        $end = $t.IndexOf('"', 1)
        if ($end -gt 1) {
            return $t.Substring(1, $end - 1)
        }
        return $t.Trim('"')
    }
    $m = [regex]::Match($t, '(?i)^(.+?\.exe)(\s|$)')
    if ($m.Success) {
        return $m.Groups[1].Value
    }
    $space = $t.IndexOf(' ')
    if ($space -gt 0) {
        return $t.Substring(0, $space)
    }
    return $t
}

# The file name of a path (both separators, whatever the platform).
function Get-FileName {
    param([string]$Path)
    $i = $Path.LastIndexOfAny([char[]]@('\', '/'))
    return $Path.Substring($i + 1)
}

# $false only when the path names a folder and the file is not there; a bare
# file name is found through PATH and is not checked.
function Test-CommandExists {
    param([string]$Path)
    $expanded = [Environment]::ExpandEnvironmentVariables($Path)
    if ($expanded.IndexOfAny([char[]]@('\', '/')) -lt 0) {
        return $true
    }
    return (Test-Path -LiteralPath $expanded -PathType Leaf)
}

# One entry per program: @{ Program; Debugger (file name); Exists }.
$found = [ordered]@{}
foreach ($program in @($logonTools + $systemTools + $securityTools)) {
    foreach ($view in $views) {
        if ($found.Contains($program)) {
            break
        }
        $command = Get-Debugger $view $program
        if ($command.Length -eq 0) {
            continue
        }
        $path = Get-CommandPath $command
        $found[$program] = [pscustomobject]@{
            Program  = $program
            Debugger = (Get-FileName $path).ToLowerInvariant()
            Exists   = (Test-CommandExists $path)
        }
    }
}

$logon = @($found.Values | Where-Object { $logonTools -contains $_.Program })
$hijacked = @($found.Values | Where-Object {
        (($systemTools -contains $_.Program) -or ($securityTools -contains $_.Program)) -and
        (-not (($_.Program -eq 'taskmgr.exe') -and ($replacers -contains $_.Debugger)))
    })
$replaced = @($found.Values | Where-Object { ($_.Program -eq 'taskmgr.exe') -and ($replacers -contains $_.Debugger) })

$result = 'ok'
$listed = @()
if ($logon.Count -gt 0) {
    $result = 'logon-backdoor'
    $listed = $logon
}
elseif ($hijacked.Count -gt 0) {
    $result = 'hijacked'
    $listed = $hijacked
}
elseif ($replaced.Count -gt 0) {
    $listed = $replaced
    $result = 'taskmgr-replaced'
    if (-not $replaced[0].Exists) {
        $result = 'taskmgr-missing'
    }
}

$facts = [ordered]@{ programs = @($listed | ForEach-Object { $_.Program }) }
if ($listed.Count -gt 0) {
    $facts['debugger'] = $listed[0].Debugger
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
