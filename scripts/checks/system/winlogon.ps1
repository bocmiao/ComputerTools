# Check: system.winlogon
# Did something replace what Windows starts after sign-in? Then the screen can
# stay black with only the mouse pointer (no desktop, no taskbar), or unknown
# programs start with every sign-in. Read-only; reads the signed-in user's
# hive for the per-user override (-UserHive).
# Result codes, in this order: userinit-broken (Userinit does not start
# userinit.exe) / shell-missing / shell-changed / userinit-extra / user-shell
# (a per-user Shell) / ok. All are fixed by system.winlogon-defaults.
# Facts: shell, user_shell (file names, '' when not set), userinit_extra
# (file names of the extra programs, comma separated), userinit_ok.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

# ---- shared block winlogon: identical in checks/system/winlogon.ps1 and features/system/winlogon-*.ps1 (medkit-data check compares them) ----
# What Windows starts after sign-in, under
# HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon (REG_SZ):
# - Shell: the desktop and taskbar, by default explorer.exe. Replaced by
#   malware, and on purpose by kiosk, internet cafe and school lab software.
# - Userinit: by default "<SystemRoot>\system32\userinit.exe," (a comma
#   separated list; userinit.exe starts Shell). Malware adds its own programs.
# - A Shell value under the same path in the user's hive overrides the
#   machine one for that user (none by default).
# Entries are compared by their full path with environment variables
# expanded, case-insensitively; only file names are ever output, since paths
# can contain the user name.
$winlogonKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'

function Get-WinlogonValue {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -ErrorAction Stop
    }
    catch {
        return $null
    }
    # A key without any values (the user's Winlogon key can be empty) gives
    # nothing.
    if ($null -eq $item) {
        return $null
    }
    $prop = $item.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    return [string]$prop.Value
}

# The program of one entry: quotes and arguments removed, environment
# variables expanded.
function Get-EntryPath {
    param([string]$Entry)
    $e = $Entry.Trim()
    if ($e.StartsWith('"')) {
        $end = $e.IndexOf('"', 1)
        if ($end -gt 0) {
            $e = $e.Substring(1, $end - 1)
        }
    }
    elseif ($e -match '^(.+?\.(exe|com|bat|cmd))(\s.*)?$') {
        $e = $Matches[1]
    }
    return [Environment]::ExpandEnvironmentVariables($e).Trim()
}

# The file name of one entry (the part after the last \ or /).
function Get-EntryName {
    param([string]$Entry)
    $path = Get-EntryPath $Entry
    $i = $path.LastIndexOfAny([char[]]@('\', '/'))
    if ($i -ge 0) {
        return $path.Substring($i + 1)
    }
    return $path
}

# True for explorer.exe, by name or as the Windows folder's own.
function Test-DefaultShell {
    param([string]$Value)
    $path = Get-EntryPath $Value
    return (($path -ieq 'explorer.exe') -or ($path -ieq 'explorer') -or ($path -ieq ($env:SystemRoot + '\explorer.exe')))
}

# The Userinit entries that are not userinit.exe of this Windows, and
# whether userinit.exe is there: @{ Extra = [string[]] (file names); HasUserinit }.
function Get-UserinitState {
    param([string]$Value)
    $own = ($env:SystemRoot + '\system32\userinit.exe')
    $extra = New-Object System.Collections.Generic.List[string]
    $has = $false
    foreach ($entry in ([string]$Value).Split(',')) {
        if ($entry.Trim().Length -eq 0) {
            continue
        }
        $path = Get-EntryPath $entry
        if (($path -ieq $own) -or ($path -ieq 'userinit.exe') -or ($path -ieq 'userinit')) {
            $has = $true
        }
        else {
            $extra.Add((Get-EntryName $entry))
        }
    }
    return @{ Extra = $extra.ToArray(); HasUserinit = $has }
}
# ---- end of shared block winlogon ----

$shell = Get-WinlogonValue $winlogonKey 'Shell'
$userinit = Get-WinlogonValue $winlogonKey 'Userinit'
$userShell = Get-WinlogonValue ($UserHive.TrimEnd('\') + '\Software\Microsoft\Windows NT\CurrentVersion\Winlogon') 'Shell'
$state = Get-UserinitState $userinit

$facts = [ordered]@{ shell = ''; user_shell = ''; userinit_extra = ($state.Extra -join ', '); userinit_ok = $state.HasUserinit }
if (-not [string]::IsNullOrWhiteSpace($shell)) {
    $facts.shell = Get-EntryName $shell
}
if (-not [string]::IsNullOrWhiteSpace($userShell)) {
    $facts.user_shell = Get-EntryName $userShell
}

$result = 'ok'
if (-not $state.HasUserinit) {
    $result = 'userinit-broken'
}
elseif ([string]::IsNullOrWhiteSpace($shell)) {
    $result = 'shell-missing'
}
elseif (-not (Test-DefaultShell $shell)) {
    $result = 'shell-changed'
}
elseif ($state.Extra.Count -gt 0) {
    $result = 'userinit-extra'
}
elseif ((-not [string]::IsNullOrWhiteSpace($userShell)) -and (-not (Test-DefaultShell $userShell))) {
    $result = 'user-shell'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
