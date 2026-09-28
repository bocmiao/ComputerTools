# Items that programs added to File Explorer: the root of the navigation pane
# (next to This PC and Network: cloud drives, OneDrive) and This PC ("WPS
# cloud", "Baidu Netdisk"). Read-only.
#   Navigation pane: <hive>\Software\Microsoft\Windows\CurrentVersion\Explorer\
#     Desktop\NameSpace\{CLSID}; it is shown when the CLSID key has
#     System.IsPinnedToNameSpaceTree = 1 (Microsoft, "Integrate a Cloud
#     Storage Provider"). Explorer reads HKEY_CLASSES_ROOT: when the user's
#     Software\Classes\CLSID\{CLSID} key is there, its values are used and the
#     machine's key's values are not ("Merged View of HKEY_CLASSES_ROOT");
#     32-bit programs (their open and save dialogs) read the key under
#     WOW6432Node.
#   This PC: <hive>\...\Explorer\MyComputer\NameSpace\{CLSID}; the logged-in
#     user hides one with a DWORD {CLSID} = 1 under
#     ...\Explorer\HideMyComputerIcons (the engine reads that itself).
# <hive> is the machine (all users) or the logged-in user (-UserHive).
# Output: result = 'ok', items = one per registration:
#   place (nav / pc), clsid, hive (machine / user: where it is registered),
#   name (the default value of the NameSpace key),
#   title and localized (the default value and LocalizedString of the CLSID
#     key, the user's first; they can be "@file,-id", the engine resolves
#     them),
#   user, machine, wow_user, wow_machine: that CLSID key in the user's and the
#     machine's classes, and under WOW6432Node: exists, pinned
#     (System.IsPinnedToNameSpaceTree, -1 when it is missing), only_pinned
#     (the key holds that value and nothing else: no other value, no subkey),
#   system_server (its InProcServer32 is in the Windows folder, or a bare file
#     name found there), folder_target (Instance\InitPropertyBag names a
#     folder: TargetFolderPath or Target, the way cloud drives register).
# The engine decides what is listed (Windows' own items are left out) and what
# to change.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$maxItems = 200
$explorer = 'Software\Microsoft\Windows\CurrentVersion\Explorer'
$pinnedName = 'System.IsPinnedToNameSpaceTree'
$noExpand = [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
$windowsDir = ([string]$env:SystemRoot).TrimEnd('\')
if ($windowsDir.Length -eq 0) {
    $windowsDir = 'C:\Windows'
}

# The logged-in user's registry: HKEY_USERS\<SID> ('Registry::HKEY_USERS\<SID>'), else this process's.
$userRoot = $null
$closeUser = $false
try {
    if ($UserHive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
        $userRoot = [Microsoft.Win32.Registry]::Users.OpenSubKey($Matches[1])
        $closeUser = $true
    }
    else {
        $userRoot = [Microsoft.Win32.Registry]::CurrentUser
    }
}
catch {
    $userRoot = $null
}
$roots = [ordered]@{ user = $userRoot; machine = [Microsoft.Win32.Registry]::LocalMachine }

function Open-Key {
    param($Root, [string]$Path)
    if ($null -eq $Root) {
        return $null
    }
    try {
        return $Root.OpenSubKey($Path)
    }
    catch {
        return $null
    }
}

# A string value ('' when it is missing or not a string).
function Get-Text {
    param($Key, [string]$Name)
    $value = $Key.GetValue($Name, $null, $noExpand)
    if ($value -is [string]) {
        return $value.Trim()
    }
    return ''
}

# A DWORD value as a number, -1 when it is missing or not a number.
function Get-Number {
    param($Key, [string]$Name)
    $value = $Key.GetValue($Name, $null)
    if ($value -is [int]) {
        return [int64]$value
    }
    return [int64]-1
}

# One CLSID key: is it there, its pinned value, does it hold nothing else.
function Get-KeyInfo {
    param($Root, [string]$Path)
    $info = [ordered]@{ exists = $false; pinned = [int64]-1; only_pinned = $false }
    $key = Open-Key $Root $Path
    if ($null -eq $key) {
        return $info
    }
    try {
        $info.exists = $true
        $info.pinned = Get-Number $key $pinnedName
        $others = @($key.GetValueNames() | Where-Object { $_ -ne $pinnedName })
        $info.only_pinned = ($info.pinned -ge 0) -and ($others.Count -eq 0) -and ($key.SubKeyCount -eq 0)
    }
    finally {
        $key.Close()
    }
    return $info
}

# A text of the CLSID key or one of its subkeys: the user's first (HKEY_CLASSES_ROOT does the same), then the machine's.
function Get-ClassText {
    param([string]$Path, [string]$Name)
    foreach ($hive in @('user', 'machine')) {
        $key = Open-Key $roots[$hive] $Path
        if ($null -eq $key) {
            continue
        }
        $text = ''
        try {
            $text = Get-Text $key $Name
        }
        finally {
            $key.Close()
        }
        if ($text.Length -gt 0) {
            return $text
        }
    }
    return ''
}

function Test-SystemServer {
    param([string]$Clsid)
    $path = Get-ClassText ('Software\Classes\CLSID\' + $Clsid + '\InProcServer32') ''
    $path = [Environment]::ExpandEnvironmentVariables($path).Trim().Trim('"').Trim()
    if ($path.Length -eq 0) {
        return $false
    }
    if ($path -notmatch '[\\/]') {
        return $true
    }
    return $path.StartsWith($windowsDir + '\', [StringComparison]::OrdinalIgnoreCase)
}

function Test-FolderTarget {
    param([string]$Clsid)
    $bag = 'Software\Classes\CLSID\' + $Clsid + '\Instance\InitPropertyBag'
    foreach ($name in @('TargetFolderPath', 'Target')) {
        if ((Get-ClassText $bag $name).Length -gt 0) {
            return $true
        }
    }
    return $false
}

$items = New-Object System.Collections.Generic.List[object]
foreach ($place in @(@('nav', ($explorer + '\Desktop\NameSpace')), @('pc', ($explorer + '\MyComputer\NameSpace')))) {
    foreach ($hive in @('machine', 'user')) {
        $key = Open-Key $roots[$hive] $place[1]
        if ($null -eq $key) {
            continue
        }
        try {
            foreach ($clsid in $key.GetSubKeyNames()) {
                if (($items.Count -ge $maxItems) -or ($clsid -notmatch '^\{[0-9A-Fa-f]{8}-([0-9A-Fa-f]{4}-){3}[0-9A-Fa-f]{12}\}$')) {
                    continue
                }
                $name = ''
                $sub = Open-Key $key $clsid
                if ($null -ne $sub) {
                    try {
                        $name = Get-Text $sub ''
                    }
                    finally {
                        $sub.Close()
                    }
                }
                $class = 'Software\Classes\CLSID\' + $clsid
                $wow = 'Software\Classes\WOW6432Node\CLSID\' + $clsid
                $items.Add([ordered]@{
                        place         = $place[0]
                        clsid         = $clsid
                        hive          = $hive
                        name          = $name
                        title         = Get-ClassText $class ''
                        localized     = Get-ClassText $class 'LocalizedString'
                        user          = Get-KeyInfo $roots.user $class
                        machine       = Get-KeyInfo $roots.machine $class
                        wow_user      = Get-KeyInfo $roots.user $wow
                        wow_machine   = Get-KeyInfo $roots.machine $wow
                        system_server = Test-SystemServer $clsid
                        folder_target = Test-FolderTarget $clsid
                    })
            }
        }
        finally {
            $key.Close()
        }
    }
}
if ($closeUser -and ($null -ne $userRoot)) {
    $userRoot.Close()
}

[pscustomobject]@{
    result = 'ok'
    items  = $items.ToArray()
}
