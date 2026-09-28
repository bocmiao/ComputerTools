# Feature: system.winlogon-defaults -- run (and prepare)
# Puts back what Windows starts after sign-in: Shell = explorer.exe,
# Userinit = "<SystemRoot>\system32\userinit.exe," (the path of this
# Windows, never a fixed drive), and removes a per-user Shell that is not
# explorer.exe. Used at the next sign-in.
# -Prepare: returns before = { shell, userinit, user_shell } (the values as
#   they are, null when missing).
# Run: -Before is that JSON. Returns skipped (nothing changed) when a value
#   changed since, or everything is the default already. Otherwise writes
#   the defaults and checks them; when that fails, the recorded values are
#   written back and the script throws.

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = '',
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

# What Windows writes: Userinit with the path of this Windows, never a fixed drive.
$defaultShell = 'explorer.exe'
$defaultUserinit = $env:SystemRoot + '\system32\userinit.exe,'

$snapshot = [ordered]@{
    shell      = Get-WinlogonValue $winlogonKey 'Shell'
    userinit   = Get-WinlogonValue $winlogonKey 'Userinit'
    user_shell = Get-WinlogonValue $userWinlogonKey 'Shell'
}
if ($Prepare) {
    return [pscustomobject]@{ before = $snapshot }
}

$recorded = ConvertFrom-Json -InputObject $Before
foreach ($name in @('shell', 'userinit', 'user_shell')) {
    if ([string]$recorded.$name -cne [string]$snapshot[$name]) {
        return [pscustomobject]@{ skipped = $true }
    }
}
if (Test-WinlogonDefault) {
    return [pscustomobject]@{ skipped = $true }
}

try {
    if ([string]::IsNullOrWhiteSpace($snapshot.shell) -or (-not (Test-DefaultShell $snapshot.shell))) {
        Set-WinlogonValue $winlogonKey 'Shell' $defaultShell
    }
    $state = Get-UserinitState $snapshot.userinit
    if ((-not $state.HasUserinit) -or ($state.Extra.Count -gt 0)) {
        Set-WinlogonValue $winlogonKey 'Userinit' $defaultUserinit
    }
    if ((-not [string]::IsNullOrWhiteSpace($snapshot.user_shell)) -and (-not (Test-DefaultShell $snapshot.user_shell))) {
        Set-WinlogonValue $userWinlogonKey 'Shell' $null
    }
    if (-not (Test-WinlogonDefault)) {
        throw 'the sign-in settings are still not the defaults'
    }
}
catch {
    $failure = $_
    try {
        Set-WinlogonValue $winlogonKey 'Shell' $recorded.shell
        Set-WinlogonValue $winlogonKey 'Userinit' $recorded.userinit
        if ($null -ne $recorded.user_shell) {
            Set-WinlogonValue $userWinlogonKey 'Shell' $recorded.user_shell
        }
    }
    catch {
        Write-Verbose ('could not write the values back: ' + $_.Exception.Message)
    }
    throw $failure
}

[pscustomobject]@{ after = [ordered]@{ ok = $true } }
