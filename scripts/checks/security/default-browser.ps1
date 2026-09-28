# Check: security.default-browser
# Which browser opens web links for the logged-in user. Read-only; the
# default is never changed here (Windows only takes that from Settings).
# Where the choice is: UrlAssociations\https (then http) under the user's
# Shell\Associations. Windows 11 25H2 writes it to UserChoiceLatest\ProgId
# (a subkey with a value ProgId; the hash is a value of UserChoiceLatest, as
# described by the author of SetUserFTA) and no longer mirrors it to the old
# UserChoice, so UserChoiceLatest is read first, then UserChoice.
# The browser's name comes from its ProgId: Application\ApplicationName
# (unless it points into a resource file, '@...'), else the product name or
# description of the program in shell\open\command, else that program's file
# name. The user's Classes are looked at before the machine's. Only the name
# and the file name are reported: the program's folder can hold the user name.
# Result codes: unset (no choice made, Windows decides) / found / missing (the
# chosen program is gone, e.g. uninstalled).
# Facts: name, file.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$UserHive = $UserHive.TrimEnd('\')

# A text value, or '' when the key or the value is missing.
function Get-Text {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop
    }
    catch {
        return ''
    }
    $value = $item.$Name
    if ($value -is [string]) {
        return $value.Trim()
    }
    return ''
}

# The ProgId chosen for web links, or ''.
function Get-ChosenProgId {
    $root = $UserHive + '\Software\Microsoft\Windows\Shell\Associations\UrlAssociations'
    foreach ($scheme in @('https', 'http')) {
        foreach ($where in @('UserChoiceLatest\ProgId', 'UserChoice')) {
            $progId = Get-Text ($root + '\' + $scheme + '\' + $where) 'ProgId'
            if ($progId.Length -gt 0) {
                return $progId
            }
        }
    }
    return ''
}

# The program file a command line starts, as written ('' when it cannot be
# told).
function Get-CommandFile {
    param([string]$Command)
    $command = $Command.Trim()
    $path = ''
    if ($command.StartsWith('"')) {
        $end = $command.IndexOf('"', 1)
        if ($end -gt 1) {
            $path = $command.Substring(1, $end - 1)
        }
    }
    else {
        $at = $command.ToLowerInvariant().IndexOf('.exe')
        if ($at -gt 0) {
            $path = $command.Substring(0, $at + 4)
        }
    }
    return $path
}

# What the ProgId points at: Exists (its class is registered and, when it
# names a program file, that file is there), Name, File.
function Get-Browser {
    param([string]$ProgId)
    foreach ($classes in @(($UserHive + '\Software\Classes'), 'HKLM:\SOFTWARE\Classes')) {
        $base = $classes + '\' + $ProgId
        if (-not (Test-Path -LiteralPath $base)) {
            continue
        }
        $name = Get-Text ($base + '\Application') 'ApplicationName'
        if ($name.StartsWith('@')) {
            $name = ''
        }
        $file = ''
        $exists = $true
        $written = Get-CommandFile (Get-Text ($base + '\shell\open\command') '(default)')
        $program = [Environment]::ExpandEnvironmentVariables($written)
        # Only a full path without variables tells whether the program is
        # gone: a bare name is looked up on PATH, and %LOCALAPPDATA% would
        # expand to this process's user, who may not be the logged-in one.
        $checkable = ($written.IndexOf('%') -lt 0) -and [System.IO.Path]::IsPathRooted($program)
        if ($program.Length -gt 0) {
            $file = [System.IO.Path]::GetFileName($program)
            if ($checkable -and (-not (Test-Path -LiteralPath $program -PathType Leaf))) {
                $exists = $false
            }
            elseif ($checkable -and ($name.Length -eq 0)) {
                $info = (Get-Item -LiteralPath $program).VersionInfo
                foreach ($candidate in @($info.ProductName, $info.FileDescription)) {
                    if (($candidate -is [string]) -and ($candidate.Trim().Length -gt 0)) {
                        $name = $candidate.Trim()
                        break
                    }
                }
            }
        }
        if ($name.Length -eq 0) {
            $name = $file
        }
        return [pscustomobject]@{ Exists = $exists; Name = $name; File = $file }
    }
    return [pscustomobject]@{ Exists = $false; Name = ''; File = '' }
}

$progId = Get-ChosenProgId
if ($progId.Length -eq 0) {
    return [pscustomobject]@{ result = 'unset'; facts = [ordered]@{ name = ''; file = '' } }
}
$browser = Get-Browser $progId
$result = 'found'
if ((-not $browser.Exists) -or ($browser.Name.Length -eq 0)) {
    $result = 'missing'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ name = $browser.Name; file = $browser.File }
}
