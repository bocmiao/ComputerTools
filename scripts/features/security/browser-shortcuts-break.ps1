# Feature: security.browser-shortcuts-clean -- break (tests only)
# Puts a browser shortcut with a web address in its arguments on the user's
# desktop: "MedkitTest Browser.lnk" -> Microsoft Edge (Google Chrome's usual
# path when Edge is not there) with --profile-directory=Default and an
# address. Replaces it when it is there already.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

# ---- shared block browser-shortcuts: identical in checks/security/browser-shortcuts.ps1 and features/security/browser-shortcuts-*.ps1 (medkit-data check compares them) ----
# Shortcuts (.lnk) of web browsers with a web address added to their command
# line ("...\chrome.exe" http://www.2345.com/?k123): the usual way bundled
# "home page" software makes the browser open its navigation site whenever it
# is started from that shortcut.
# Where: the logged-in user's Desktop, Start menu (Programs) and Quick Launch
# (it holds the taskbar pins, User Pinned\TaskBar), the Public Desktop and the
# Start menu of all users. The user's folders come from Shell Folders under
# -UserHive (else the profile folder in ProfileList), those of all users from
# the machine's Shell Folders. Nothing else is searched; at most
# $shortcutLimit shortcuts are opened.
# A shortcut counts when its target is one of $browserPrograms and a word of
# its arguments is a web address: it starts with http://, https:// or www., or
# it is a domain name (labels of letters, digits and hyphens ending in one of
# $webTlds, then maybe a port and a path). Switches (words starting with - or
# /, like --profile-directory=Default, or an --app=https://... made on
# purpose) never count. Only those words are removed; the other words stay,
# in their order and with their quotes.
# Shortcuts are read and written with WScript.Shell (the shell's own
# IShellLink), which keeps everything else in the file. A read-only shortcut
# is made writable for the change and read-only again after.
# A record names a shortcut by its folder id and its path inside that folder,
# never by its full path (that holds the user name).
$browserPrograms = @(
    'chrome.exe', 'msedge.exe', 'firefox.exe', 'iexplore.exe', '360se.exe', '360chrome.exe', '360chromex.exe',
    'qqbrowser.exe', 'sogouexplorer.exe', '2345explorer.exe', 'liebao.exe', 'ucbrowser.exe', 'maxthon.exe',
    'opera.exe', 'brave.exe', 'vivaldi.exe', 'chromium.exe', 'theworld.exe', 'baidubrowser.exe'
)
$webTlds = 'com|cn|net|org|cc|top|xyz|info|vip|site|me|tv|co|io|biz|wang|club|online|shop|ltd|link|pro|work|ink|red|fun|icu|live|app|store|tech|asia|mobi|hk|tw|la'
$shortcutLimit = 3000

# An absolute path from a Shell Folders value, or ''.
function Get-ShellFolder {
    param([string]$Key, [string]$Name)
    $value = ''
    try {
        $value = [string](Get-ItemProperty -LiteralPath $Key -Name $Name -ErrorAction Stop).$Name
    }
    catch {
        $value = ''
    }
    if (($value.Length -gt 0) -and ($value -notmatch '%') -and [IO.Path]::IsPathRooted($value)) {
        return $value
    }
    return ''
}

# The folders searched: Id, Path, Recurse. Folders that do not exist are left out.
function Get-ShortcutFolders {
    param([string]$Hive)
    $userKey = $Hive.TrimEnd('\') + '\Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
    $machineKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
    $profilePath = ''
    if ($Hive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
        $listKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $Matches[1]
        try {
            $profilePath = [Environment]::ExpandEnvironmentVariables([string](Get-ItemProperty -LiteralPath $listKey -Name 'ProfileImagePath' -ErrorAction Stop).ProfileImagePath)
        }
        catch {
            $profilePath = ''
        }
    }
    else {
        $profilePath = [string]$env:USERPROFILE
    }
    $desktop = Get-ShellFolder $userKey 'Desktop'
    $programs = Get-ShellFolder $userKey 'Programs'
    $appData = Get-ShellFolder $userKey 'AppData'
    if ($profilePath.Length -gt 0) {
        if ($desktop.Length -eq 0) {
            $desktop = Join-Path $profilePath 'Desktop'
        }
        if ($appData.Length -eq 0) {
            $appData = Join-Path $profilePath 'AppData\Roaming'
        }
    }
    if (($programs.Length -eq 0) -and ($appData.Length -gt 0)) {
        $programs = Join-Path $appData 'Microsoft\Windows\Start Menu\Programs'
    }
    $quickLaunch = ''
    if ($appData.Length -gt 0) {
        $quickLaunch = Join-Path $appData 'Microsoft\Internet Explorer\Quick Launch'
    }
    $publicDesktop = Get-ShellFolder $machineKey 'Common Desktop'
    if ($publicDesktop.Length -eq 0) {
        $publicDesktop = [Environment]::GetFolderPath('CommonDesktopDirectory')
    }
    $commonPrograms = Get-ShellFolder $machineKey 'Common Programs'
    if ($commonPrograms.Length -eq 0) {
        $commonPrograms = [Environment]::GetFolderPath('CommonPrograms')
    }
    $folders = New-Object System.Collections.Generic.List[object]
    $seen = @{}
    foreach ($entry in @(
            @('desktop', $desktop, $false), @('start-menu', $programs, $true), @('quick-launch', $quickLaunch, $true),
            @('public-desktop', $publicDesktop, $false), @('common-start-menu', $commonPrograms, $true))) {
        $path = ([string]$entry[1]).TrimEnd('\')
        if (($path.Length -eq 0) -or $seen.ContainsKey($path.ToLowerInvariant()) -or -not (Test-Path -LiteralPath $path -PathType Container)) {
            continue
        }
        $seen[$path.ToLowerInvariant()] = $true
        $folders.Add([pscustomobject]@{ Id = $entry[0]; Path = $path; Recurse = $entry[2] })
    }
    return $folders.ToArray()
}

function Test-WebAddress {
    param([string]$Word)
    $text = $Word.Trim('"').Trim()
    if (($text.Length -eq 0) -or $text.StartsWith('-') -or $text.StartsWith('/')) {
        return $false
    }
    if ($text -match '^(?i)(https?://|www\.)\S') {
        return $true
    }
    return ($text -match ('^(?i)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+(' + $webTlds + ')(:\d{1,5})?([/?#]\S*)?$'))
}

# The words of a command line (a quoted part belongs to its word): Urls (the
# web addresses, without quotes) and Rest (the other words, joined by spaces).
function Split-ShortcutArguments {
    param([string]$Text)
    $urls = New-Object System.Collections.Generic.List[string]
    $rest = New-Object System.Collections.Generic.List[string]
    foreach ($m in [regex]::Matches($Text, '(?:"[^"]*"|[^\s"])+')) {
        if (Test-WebAddress $m.Value) {
            $urls.Add($m.Value.Trim('"'))
        }
        else {
            $rest.Add($m.Value)
        }
    }
    return [pscustomobject]@{ Urls = $urls.ToArray(); Rest = ($rest.ToArray() -join ' ') }
}

# The host name of a web address, lower case.
function Get-WebHost {
    param([string]$Address)
    $m = [regex]::Match($Address, '^(?i)(?:https?://)?([^/?#:\s]+)')
    if ($m.Success) {
        return $m.Groups[1].Value.ToLowerInvariant()
    }
    return ''
}

function Get-ShortcutFiles {
    param($Folder)
    try {
        if ($Folder.Recurse) {
            return @(Get-ChildItem -LiteralPath $Folder.Path -Filter '*.lnk' -File -Recurse -Depth 4 -ErrorAction SilentlyContinue)
        }
        return @(Get-ChildItem -LiteralPath $Folder.Path -Filter '*.lnk' -File -ErrorAction SilentlyContinue)
    }
    catch {
        return @()
    }
}

# The browser shortcuts with web addresses in their arguments: Folder (id),
# Relative (the path inside the folder), Path, Program, Arguments, Rest, Urls.
function Get-HijackedShortcuts {
    param($Shell, [string]$Hive)
    $found = New-Object System.Collections.Generic.List[object]
    $opened = 0
    foreach ($folder in @(Get-ShortcutFolders $Hive)) {
        foreach ($file in @(Get-ShortcutFiles $folder)) {
            if ($opened -ge $shortcutLimit) {
                break
            }
            $opened++
            $target = ''
            $arguments = ''
            try {
                $link = $Shell.CreateShortcut($file.FullName)
                $target = [string]$link.TargetPath
                $arguments = [string]$link.Arguments
            }
            catch {
                continue
            }
            if ($arguments.Trim().Length -eq 0) {
                continue
            }
            $program = ([string]@($target -split '[\\/]')[-1]).ToLowerInvariant()
            if ($browserPrograms -notcontains $program) {
                continue
            }
            $split = Split-ShortcutArguments $arguments
            if ($split.Urls.Count -eq 0) {
                continue
            }
            $found.Add([pscustomobject]@{
                    Folder    = $folder.Id
                    Relative  = $file.FullName.Substring($folder.Path.Length).TrimStart([char[]]@('\', '/'))
                    Path      = $file.FullName
                    Program   = $program
                    Arguments = $arguments
                    Rest      = $split.Rest
                    Urls      = $split.Urls
                })
        }
    }
    return $found.ToArray()
}

# The full path of a recorded shortcut (folder id and relative path), or ''
# when the folder is gone or the record points outside it.
function Resolve-ShortcutRecord {
    param([string]$Hive, [string]$Folder, [string]$Relative)
    if (($Relative.Length -eq 0) -or [IO.Path]::IsPathRooted($Relative) -or ($Relative -match '(^|[\\/])\.\.([\\/]|$)') -or ($Relative -notmatch '(?i)\.lnk$')) {
        return ''
    }
    foreach ($candidate in @(Get-ShortcutFolders $Hive)) {
        if ($candidate.Id -eq $Folder) {
            return (Join-Path $candidate.Path $Relative)
        }
    }
    return ''
}

# Changes the arguments of the shortcut at Path from Expected to Value.
# $false (nothing changed) when it is gone or its arguments are not Expected.
function Set-ShortcutArguments {
    param($Shell, [string]$Path, [string]$Expected, [string]$Value)
    if (($Path.Length -eq 0) -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }
    $link = $Shell.CreateShortcut($Path)
    if ([string]$link.Arguments -ne $Expected) {
        return $false
    }
    $item = Get-Item -LiteralPath $Path -Force
    $readOnly = $item.IsReadOnly
    if ($readOnly) {
        $item.IsReadOnly = $false
    }
    try {
        $link.Arguments = $Value
        $link.Save()
    }
    finally {
        if ($readOnly) {
            (Get-Item -LiteralPath $Path -Force).IsReadOnly = $true
        }
    }
    if ([string]$Shell.CreateShortcut($Path).Arguments -ne $Value) {
        throw 'The shortcut did not take the new arguments'
    }
    return $true
}
# ---- end of shared block browser-shortcuts ----

$desktop = @(Get-ShortcutFolders $UserHive | Where-Object { $_.Id -eq 'desktop' })
if ($desktop.Count -eq 0) {
    throw 'The desktop folder was not found'
}
$programFiles = [string]${env:ProgramFiles(x86)}
if ($programFiles.Length -eq 0) {
    $programFiles = [string]$env:ProgramFiles
}
$target = $programFiles.TrimEnd('\') + '\Microsoft\Edge\Application\msedge.exe'
if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
    $target = ([string]$env:ProgramFiles).TrimEnd('\') + '\Google\Chrome\Application\chrome.exe'
}
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path $desktop[0].Path 'MedkitTest Browser.lnk'))
$link.TargetPath = $target
$link.Arguments = '--profile-directory=Default http://www.2345.com/?k20260927'
$link.Save()
[pscustomobject]@{ result = 'ok' }
