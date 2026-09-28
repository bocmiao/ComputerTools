# Feature: system.shell-folders-default -- undo
# Points the recorded folders (-Before) back where they pointed before, if
# they still point to their default place. A default folder the run made is
# removed again when it is still empty.

[CmdletBinding()]
param(
    [string]$Before = '',
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

# ---- shared block shell-folders: identical in checks/system/shell-folders.ps1 and features/system/shell-folders-*.ps1 (medkit-data check compares them) ----
# The user's Desktop, Documents, Downloads, Pictures, Music and Videos: the
# REG_EXPAND_SZ values under the logged-in user's
# Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders (what
# the Location tab of the folder's properties changes), read without
# expanding them. %USERPROFILE% is the user's own profile folder (ProfileList
# of the SID in -UserHive: medkit may run as another administrator), %OneDrive%,
# %OneDriveConsumer% and %OneDriveCommercial% come from the user's own
# Environment key; other variables are expanded as usual. The older
# "Shell Folders" key holds the expanded paths for old programs and is kept in
# step. A folder is broken when its path cannot be reached: the folder, or the
# drive it is on, is not there (OneDrive was unlinked, a drive was removed or
# got another letter). Network paths are not checked (they can be slow, or
# offline on purpose).
$shellFolders = @(
    @('desktop', 'Desktop', 'Desktop'),
    @('documents', 'Personal', 'Documents'),
    @('downloads', '{374DE290-123F-4565-9164-39C4925E467B}', 'Downloads'),
    @('pictures', 'My Pictures', 'Pictures'),
    @('music', 'My Music', 'Music'),
    @('videos', 'My Video', 'Videos')
)
$userShellKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'

# The profile folder of the user whose hive is -UserHive.
function Get-ProfilePath {
    param([string]$Hive)
    if ($Hive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
        $listKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $Matches[1]
        $item = Get-ItemProperty -LiteralPath $listKey -Name 'ProfileImagePath' -ErrorAction Stop
        return [Environment]::ExpandEnvironmentVariables([string]$item.ProfileImagePath).TrimEnd('\')
    }
    return ([string]$env:USERPROFILE).TrimEnd('\')
}

# A value as stored (REG_EXPAND_SZ not expanded), or $null.
function Get-RawValue {
    param([string]$Key, [string]$Name)
    try {
        $item = Get-Item -LiteralPath $Key -ErrorAction Stop
        $value = $item.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    }
    catch {
        return $null
    }
    if ($null -eq $value) {
        return $null
    }
    return [string]$value
}

# The user's OneDrive variables.
function Get-UserVariables {
    param([string]$Hive, [string]$ProfilePath)
    $vars = @{ 'USERPROFILE' = $ProfilePath }
    foreach ($name in @('OneDrive', 'OneDriveConsumer', 'OneDriveCommercial')) {
        $value = Get-RawValue ($Hive + '\Environment') $name
        if (($null -ne $value) -and ($value.Trim().Length -gt 0)) {
            $vars[$name.ToUpperInvariant()] = $value.Trim()
        }
    }
    return $vars
}

# A stored path with the user's variables put in, or '' when a variable is left.
function Expand-UserPath {
    param([string]$Raw, [hashtable]$Variables)
    $path = $Raw
    foreach ($m in [regex]::Matches($Raw, '%([^%]+)%')) {
        $name = $m.Groups[1].Value.ToUpperInvariant()
        if ($Variables.ContainsKey($name)) {
            $path = $path.Replace($m.Value, $Variables[$name])
        }
    }
    $path = [Environment]::ExpandEnvironmentVariables($path)
    if ($path.Contains('%')) {
        return ''
    }
    return $path.Trim()
}

# ok / missing / network / unknown (not a path starting with a drive letter, or
# a variable left).
function Get-PathState {
    param([string]$Path)
    if ($Path.Length -eq 0) {
        return 'unknown'
    }
    if ($Path.StartsWith('\\')) {
        return 'network'
    }
    if ($Path -notmatch '^[A-Za-z]:\\') {
        return 'unknown'
    }
    if (Test-Path -LiteralPath $Path -PathType Container) {
        return 'ok'
    }
    return 'missing'
}

# One entry per folder: @{ Code; Name; DefaultRaw; DefaultPath; Raw; Path;
# State; Kind ('onedrive' / 'drive' / 'folder', for a missing one); Moved }.
function Get-ShellFolderStates {
    param([string]$Hive)
    $profilePath = Get-ProfilePath $Hive
    $variables = Get-UserVariables $Hive $profilePath
    $key = $Hive.TrimEnd('\') + '\' + $userShellKey
    $states = New-Object System.Collections.Generic.List[object]
    foreach ($folder in $shellFolders) {
        $defaultRaw = '%USERPROFILE%\' + $folder[2]
        $defaultPath = $profilePath + '\' + $folder[2]
        $raw = Get-RawValue $key $folder[1]
        if (($null -eq $raw) -or ($raw.Trim().Length -eq 0)) {
            $raw = $defaultRaw
        }
        $path = Expand-UserPath $raw $variables
        $state = Get-PathState $path
        $kind = ''
        if ($state -eq 'missing') {
            $kind = 'folder'
            if (($raw -match '(?i)%onedrive') -or ($path -match '(?i)\\onedrive( - [^\\]+)?(\\|$)')) {
                $kind = 'onedrive'
            }
            elseif (-not (Test-Path -LiteralPath $path.Substring(0, 3) -PathType Container)) {
                $kind = 'drive'
            }
        }
        $states.Add([pscustomobject]@{
                Code        = $folder[0]
                Name        = $folder[1]
                DefaultRaw  = $defaultRaw
                DefaultPath = $defaultPath
                Raw         = $raw
                Path        = $path
                State       = $state
                Kind        = $kind
                Moved       = -not [string]::Equals($path.TrimEnd('\'), $defaultPath, [StringComparison]::OrdinalIgnoreCase)
            })
    }
    return , $states.ToArray()
}
# ---- end of shared block shell-folders ----

$shellKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders'
$userPath = $UserHive.TrimEnd('\') + '\' + $userShellKey
$shellPath = $UserHive.TrimEnd('\') + '\' + $shellKey

# Puts back what a folder pointed to before (both keys; a Shell Folders value
# that was not there is removed again).
function Restore-Folder {
    param($Record)
    $folder = @($shellFolders | Where-Object { $_[0] -eq [string]$Record.code })[0]
    Set-ItemProperty -LiteralPath $userPath -Name $folder[1] -Value ([string]$Record.user) -Type ExpandString -ErrorAction Stop
    if ($null -eq $Record.shell) {
        Remove-ItemProperty -LiteralPath $shellPath -Name $folder[1] -ErrorAction SilentlyContinue
    }
    else {
        Set-ItemProperty -LiteralPath $shellPath -Name $folder[1] -Value ([string]$Record.shell) -Type String -ErrorAction Stop
    }
}

$recorded = @(@((ConvertFrom-Json -InputObject $Before).folders) | Where-Object { $null -ne $_ })
$states = Get-ShellFolderStates $UserHive
foreach ($record in $recorded) {
    $now = @($states | Where-Object { $_.Code -eq [string]$record.code })
    if (($now.Count -eq 0) -or (-not [string]::Equals($now[0].Raw, $now[0].DefaultRaw, [StringComparison]::OrdinalIgnoreCase))) {
        continue
    }
    Restore-Folder $record
    if ((-not [bool]$record.default_existed) -and (Test-Path -LiteralPath $now[0].DefaultPath -PathType Container)) {
        if (@(Get-ChildItem -LiteralPath $now[0].DefaultPath -Force -ErrorAction Stop).Count -eq 0) {
            Remove-Item -LiteralPath $now[0].DefaultPath -Force -ErrorAction Stop
        }
    }
}

[pscustomobject]@{ result = 'ok' }
