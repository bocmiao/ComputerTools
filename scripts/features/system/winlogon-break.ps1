# Feature: system.winlogon-defaults -- break (tests only)
# Adds a program that does not exist to Userinit, the way malware adds its
# own: harmless even at a sign-in (starting a missing file just fails).

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

$userWinlogonKey = $UserHive.TrimEnd('\') + '\Software\Microsoft\Windows NT\CurrentVersion\Winlogon'

# True when Shell, Userinit and the per-user Shell are what Windows uses.
function Test-WinlogonDefault {
    $shell = Get-WinlogonValue $winlogonKey 'Shell'
    $state = Get-UserinitState (Get-WinlogonValue $winlogonKey 'Userinit')
    $userShell = Get-WinlogonValue $userWinlogonKey 'Shell'
    return ((-not [string]::IsNullOrWhiteSpace($shell)) -and (Test-DefaultShell $shell) -and $state.HasUserinit -and ($state.Extra.Count -eq 0) -and ([string]::IsNullOrWhiteSpace($userShell) -or (Test-DefaultShell $userShell)))
}

# Writes a REG_SZ value, or removes it when $Value is $null.
function Set-WinlogonValue {
    param([string]$Path, [string]$Name, $Value)
    if ($null -eq $Value) {
        Remove-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction SilentlyContinue
    }
    else {
        Set-ItemProperty -LiteralPath $Path -Name $Name -Value ([string]$Value) -Type String -ErrorAction Stop
    }
}

$own = $env:SystemRoot + '\system32\userinit.exe,'
Set-WinlogonValue $winlogonKey 'Userinit' ($own + $env:SystemRoot + '\system32\medkit-test-missing.exe,')

[pscustomobject]@{ result = 'ok' }
