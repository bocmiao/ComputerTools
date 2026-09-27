# Tool: disk.usage
# What takes up the system drive: the size of the usual big items, as tables
# for the "info" tool view (docs/architecture.md 11.2), and which of them is the
# biggest, so that the YAML can say where to clean it up. Read-only: nothing is
# deleted, changed or written, no permission is changed, nothing goes over the
# network. Needs administrator rights (system folders, other users' recycle
# bins) and the logged-on user's registry (-UserHive).
#
# Measured, on the system drive only:
#   system   hiberfil (hiberfil.sys), pagefile (pagefile.sys + swapfile.sys),
#            user_temp (the user's TEMP, from <hive>\Environment), windows_temp
#            (%SystemRoot%\Temp), update_cache (SoftwareDistribution\Download),
#            delivery_optimization (%SystemRoot%\ServiceProfiles\NetworkService\
#            AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache, the
#            default location), windows_old (Windows.old), memory_dump
#            (CrashControl DumpFile + MinidumpDir, default %SystemRoot%\MEMORY.DMP
#            and %SystemRoot%\Minidump), recycle_bin ($Recycle.Bin, all users,
#            nothing inside is listed)
#   user     desktop, documents (without the chat folders below), downloads,
#            pictures, videos: where User Shell Folders in the user's hive puts
#            them; one that is not on the system drive shows "elsewhere"
#   chat     wechat3 (<Documents>\WeChat Files), wechat4 (<Documents>\
#            xwechat_files), qq (<Documents>\Tencent Files): default locations
#            only, both under the Documents folder that User Shell Folders
#            names and under <profile>\Documents; a folder that is not there is
#            not shown
# The user's profile folder comes from ProfileList\<SID>\ProfileImagePath (the
# SID is the one in -UserHive); with -UserHive 'HKCU:' (development) it is
# $env:USERPROFILE. %VAR% in registry paths is expanded with the user's values
# (USERPROFILE, <hive>\Volatile Environment, <hive>\Environment), never with
# those of the administrator running the tool.
#
# Counting: .NET DirectoryInfo enumeration with an explicit stack, one
# directory at a time, each in try/catch (a folder that cannot be read is
# skipped and counted in "skipped"). Every size is the sum of file lengths, as
# Explorer's "Size" (hard links, for example in Windows.old\Windows\WinSxS, are
# counted more than once; compression and cluster slack are ignored).
#   - Junctions, symbolic links and mount points are never followed (no loops,
#     no double counting): a folder with a reparse point is left out when its
#     LinkType (PowerShell's extended property: Junction / SymbolicLink) says
#     so or cannot be read. Cloud files (OneDrive) are reparse points too, and
#     Windows shows them as such to processes under %SystemRoot% such as
#     powershell.exe; they have no LinkType and are counted.
#   - Files and folders that are not on the disk are not counted: attribute
#     OFFLINE 0x1000, RECALL_ON_OPEN 0x40000 or RECALL_ON_DATA_ACCESS 0x400000
#     (OneDrive's online-only files). Such folders are not entered either, so
#     that no sync engine is asked to fetch a folder listing.
#   - A folder that is itself a link, or lies below one, is measured where the
#     link points when that is on the system drive, and shown as "elsewhere"
#     when it is not.
#   - Every folder is measured once: a folder that another entry measures
#     (for example WeChat Files inside Documents) is skipped by the others.
# Time budget: $BudgetMs (60 s) for the whole script. The folders take turns of
# at most $turnMs; a turn is the fair share of the time left, so small folders
# finish at once and big ones share the rest; no folder gets more than half of
# the budget in total. A directory that is half read when a turn ends is
# continued (its enumerator is kept), so nothing is read twice. What is not
# finished shows the size counted so far with an "_partial" row id ("at least,
# not finished"), or not-counted when nothing was counted.
#
# Privacy: the output has fixed ids and codes, sizes and counts only: no path,
# no file or folder name, no user name. The only name is the drive letter.
# Error messages have the profile path, the user name and the computer name
# replaced.
#
# Output (ids and codes are labelled in the YAML):
#   drive    total, used, free; note rows: unfinished (something was not
#            finished in time), no-profile (the user's folders were not found)
#   system / user / chat   one row per item that is on the system drive, from
#            the biggest to the smallest ("<id>" or "<id>_partial" with a size,
#            or code elsewhere / unknown / not-counted); the chat table has a
#            note row wechat-both when both WeChat folders are there. These
#            three tables are ordered by their total size.
#   advice   the other groups of at least 1 GB (after the biggest one, whose
#            advice is the result's "next"): row = group, code advice-<group>
# Groups (pagefile is shown but never advised on): cleanup (user_temp,
# windows_temp, update_cache, delivery_optimization, memory_dump), recycle_bin,
# hiberfil (code hibernation), windows_old, chat, user_files.
# Result codes: top-<group code> when the biggest group has at least 1 GB, else
# ok. Facts: drive, total_gb, used_gb, free_gb, free_pct, top_gb, listed_gb,
# unfinished, skipped, entries, seconds.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:',
    # Only the tests pass this; the engine passes -UserHive only.
    [ValidateRange(1, 300000)]
    [int]$BudgetMs = 60000
)

$ErrorActionPreference = 'Stop'

$watch = [System.Diagnostics.Stopwatch]::StartNew()
$invariant = [System.Globalization.CultureInfo]::InvariantCulture

# Longest turn of one folder, shortest turn, and the most one folder may get.
$turnMs = 1000
$minTurnMs = 50
$itemCapMs = [long]($BudgetMs / 2)

$directoryBit = 0x10
$reparseBit = 0x400
# OFFLINE, RECALL_ON_OPEN, RECALL_ON_DATA_ACCESS: the data is not on the disk.
$notLocalMask = 0x441000
$bigGroupBytes = 1GB

$separators = [char[]]@('\', '/')

$items = New-Object System.Collections.Generic.List[object]
# Every folder that is measured, by its real path: each is measured once.
$roots = New-Object 'System.Collections.Generic.HashSet[string]' -ArgumentList ([System.StringComparer]::OrdinalIgnoreCase)
# %NAME% values for registry paths.
$expandVars = New-Object 'System.Collections.Generic.Dictionary[string,string]' -ArgumentList ([System.StringComparer]::OrdinalIgnoreCase)
$profilePath = ''

# ---- helpers

# Error text without the profile path, the user name and the computer name.
function Protect-Message {
    param([string]$Message)
    $secrets = New-Object System.Collections.Generic.List[string]
    foreach ($s in @($profilePath, $env:USERPROFILE)) {
        if (-not [string]::IsNullOrEmpty($s)) {
            $secrets.Add([string]$s)
            $secrets.Add([System.IO.Path]::GetFileName(([string]$s).TrimEnd($separators)))
        }
    }
    $secrets.Add([string]$env:USERNAME)
    $secrets.Add([string]$env:COMPUTERNAME)
    foreach ($s in $secrets) {
        if ($s.Length -ge 3) {
            $Message = [regex]::Replace($Message, [regex]::Escape($s), '<hidden>', [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        }
    }
    return $Message
}

# Raw text of a registry value (REG_EXPAND_SZ left unexpanded); '' when the key
# or the value is missing or cannot be read.
function Get-RegistryText {
    param([string]$Path, [string]$Name)
    try {
        $key = Get-Item -LiteralPath $Path
    }
    catch {
        return ''
    }
    try {
        $value = $key.GetValue($Name, $null, 'DoNotExpandEnvironmentNames')
        if ($null -eq $value) {
            return ''
        }
        return ([string]$value).Trim()
    }
    catch {
        return ''
    }
    finally {
        $key.Close()
    }
}

# Adds every string value of a registry key to the %NAME% table.
function Add-RegistryVariable {
    param([string]$Path)
    try {
        $key = Get-Item -LiteralPath $Path
    }
    catch {
        return
    }
    try {
        foreach ($name in @($key.GetValueNames())) {
            $value = $key.GetValue($name, $null, 'DoNotExpandEnvironmentNames')
            if ((-not [string]::IsNullOrEmpty($name)) -and ($value -is [string]) -and ($value.Length -gt 0)) {
                $expandVars[$name] = $value
            }
        }
    }
    catch {
        return
    }
    finally {
        $key.Close()
    }
}

# %NAME% replaced from $expandVars (values may hold %NAME% themselves); '' when
# a name is unknown.
function Expand-PathText {
    param([string]$Text)
    $out = $Text
    for ($pass = 0; $pass -lt 4; $pass++) {
        $found = [regex]::Matches($out, '%([^%\\/]+)%')
        if ($found.Count -eq 0) {
            return $out
        }
        $sb = New-Object System.Text.StringBuilder
        $pos = 0
        foreach ($m in $found) {
            $name = $m.Groups[1].Value
            if (-not $expandVars.ContainsKey($name)) {
                return ''
            }
            $null = $sb.Append($out.Substring($pos, $m.Index - $pos)).Append($expandVars[$name])
            $pos = $m.Index + $m.Length
        }
        $null = $sb.Append($out.Substring($pos))
        $out = $sb.ToString()
    }
    return ''
}

# Full path without a trailing separator (a drive root keeps it); '' when the
# text is not an absolute path.
function Resolve-FullPath {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return ''
    }
    $text = $Path.Trim()
    try {
        $root = [System.IO.Path]::GetPathRoot($text)
        $absolute = ($root -match '^[A-Za-z]:[\\/]$') -or ($root -match '^[\\/]{2}[^\\/]') -or (($root -eq '/') -and ([System.IO.Path]::DirectorySeparatorChar -eq '/'))
        if (-not $absolute) {
            return ''
        }
        $full = [System.IO.Path]::GetFullPath($text)
    }
    catch {
        return ''
    }
    $trimmed = $full.TrimEnd($separators)
    if (($trimmed.Length -eq 0) -or $trimmed.EndsWith(':')) {
        return $full
    }
    return $trimmed
}

function Join-Folder {
    param([string]$Base, [string[]]$Names)
    if ($Base.Length -eq 0) {
        return ''
    }
    $path = $Base
    foreach ($n in $Names) {
        $path = [System.IO.Path]::Combine($path, $n)
    }
    return $path
}

function Test-OnSystemDrive {
    param([string]$Path)
    if ($Path.Length -eq 0) {
        return $false
    }
    return ($Path.StartsWith($systemRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        [string]::Equals($Path.TrimEnd($separators), $systemRoot.TrimEnd($separators), [System.StringComparison]::OrdinalIgnoreCase))
}

# 'Junction', 'SymbolicLink' or '' (also for OneDrive placeholders). This is
# the method behind PowerShell's LinkType property (types.ps1xml), called
# directly: reading the property hides errors (a getter that throws just gives
# $null), and here an error has to be seen.
function Get-LinkType {
    param($Directory)
    return [string][Microsoft.PowerShell.Commands.InternalSymbolicLinkLinkCodeMethods]::GetLinkType($Directory)
}

# $true when a folder with a reparse point is a junction, a symbolic link or a
# mount point, or when that cannot be told: such folders are never followed.
# Cloud-file placeholders (OneDrive) have no LinkType and are ordinary folders.
function Test-LinkDirectory {
    param($Directory)
    try {
        $type = Get-LinkType $Directory
    }
    catch {
        return $true
    }
    return ($type.Length -gt 0)
}

# Where a junction or symbolic link points, as a full path. '' when unknown,
# '<volume>' for a mount point of another volume.
function Get-LinkTarget {
    param($Info)
    $target = ''
    try {
        foreach ($t in @($Info.Target)) {
            if ($null -ne $t) {
                $target = [string]$t
                break
            }
        }
    }
    catch {
        return ''
    }
    if ($target.StartsWith('\??\')) {
        $target = $target.Substring(4)
    }
    if ($target.StartsWith('\\?\UNC\', [System.StringComparison]::OrdinalIgnoreCase)) {
        $target = '\\' + $target.Substring(8)
    }
    elseif ($target.StartsWith('\\?\')) {
        $target = $target.Substring(4)
    }
    if ($target -match '^(?i)Volume\{') {
        return '<volume>'
    }
    if ($target.Length -eq 0) {
        return ''
    }
    try {
        if (-not [System.IO.Path]::IsPathRooted($target)) {
            $target = [System.IO.Path]::Combine([System.IO.Path]::GetDirectoryName($Info.FullName), $target)
        }
    }
    catch {
        return ''
    }
    return (Resolve-FullPath $target)
}

# Follows the junctions and symbolic links on the way to $Path, a full path on
# the system drive. State: local (Path is the real folder), elsewhere (a link
# leads off the system drive; Self tells whether the folder itself is that
# link), missing, or unknown.
function Resolve-PhysicalPath {
    param([string]$Path)
    $current = $Path
    for ($hop = 0; $hop -lt 8; $hop++) {
        if (-not (Test-OnSystemDrive $current)) {
            return @{ State = 'elsewhere'; Self = $false; Path = '' }
        }
        $parts = $current.Substring([math]::Min($current.Length, $systemRoot.Length)).Split($separators, [System.StringSplitOptions]::RemoveEmptyEntries)
        $prefix = $systemRoot
        $next = ''
        for ($i = 0; $i -lt $parts.Length; $i++) {
            $prefix = [System.IO.Path]::Combine($prefix, $parts[$i])
            $info = New-Object System.IO.DirectoryInfo -ArgumentList $prefix
            # -1: does not exist; 0: could not be read (a failing property
            # getter gives $null in PowerShell).
            try {
                $attr = [int]$info.Attributes
            }
            catch {
                $attr = 0
            }
            if ($attr -eq -1) {
                return @{ State = 'missing'; Self = $false; Path = '' }
            }
            if ($attr -eq 0) {
                return @{ State = 'unknown'; Self = $false; Path = '' }
            }
            if ((($attr -band $reparseBit) -ne 0) -and (Test-LinkDirectory $info)) {
                $target = Get-LinkTarget $info
                $self = ($i -eq ($parts.Length - 1))
                if ($target -eq '<volume>') {
                    return @{ State = 'elsewhere'; Self = $self; Path = '' }
                }
                if ($target.Length -eq 0) {
                    return @{ State = 'unknown'; Self = $false; Path = '' }
                }
                if (-not (Test-OnSystemDrive $target)) {
                    return @{ State = 'elsewhere'; Self = $self; Path = '' }
                }
                $rest = @()
                if ($i -lt ($parts.Length - 1)) {
                    $rest = $parts[($i + 1)..($parts.Length - 1)]
                }
                $next = Join-Folder -Base $target -Names $rest
                break
            }
            if (($attr -band $directoryBit) -eq 0) {
                return @{ State = 'missing'; Self = $false; Path = '' }
            }
        }
        if ($next.Length -eq 0) {
            return @{ State = 'local'; Self = $false; Path = $current }
        }
        $current = Resolve-FullPath $next
        if ($current.Length -eq 0) {
            break
        }
    }
    return @{ State = 'unknown'; Self = $false; Path = '' }
}

function New-UsageItem {
    param([string]$Id, [string]$Section)
    $item = @{
        Id         = $Id
        Section    = $Section
        Stack      = New-Object 'System.Collections.Generic.Stack[object]'
        Enumerator = $null
        Bytes      = [long]0
        Entries    = [long]0
        Skipped    = 0
        Links      = 0
        SpentMs    = [long]0
        Done       = $false
        Found      = $false
        Elsewhere  = $false
        Unknown    = $false
    }
    $items.Add($item)
    return $item
}

# Registers a folder to measure. -Optional (system and chat folders): a path
# that is not on the system drive or cannot be looked at is not shown, and
# "elsewhere" only when the folder itself is a link to another drive (it was
# moved with mklink). Otherwise (the user's folders and TEMP) such a folder
# shows elsewhere / unknown.
function Add-Folder {
    param([hashtable]$Item, [string]$Path, [switch]$Optional)
    if ($Path.Length -eq 0) {
        if (-not $Optional) {
            $Item.Unknown = $true
        }
        return
    }
    if (-not (Test-OnSystemDrive $Path)) {
        if (-not $Optional) {
            $Item.Elsewhere = $true
        }
        return
    }
    $where = Resolve-PhysicalPath $Path
    if ($where.State -eq 'elsewhere') {
        if ((-not $Optional) -or $where.Self) {
            $Item.Elsewhere = $true
        }
        return
    }
    if ($where.State -eq 'unknown') {
        if (-not $Optional) {
            $Item.Unknown = $true
        }
        return
    }
    if ($where.State -ne 'local') {
        return
    }
    if (-not $roots.Add($where.Path)) {
        return
    }
    $Item.Stack.Push((New-Object System.IO.DirectoryInfo -ArgumentList $where.Path))
    $Item.Found = $true
}

# Adds the size of one file (hiberfil.sys, pagefile.sys, MEMORY.DMP).
# FileInfo falls back to FindFirstFile for files that are in use.
function Add-File {
    param([hashtable]$Item, [string]$Path)
    if (($Path.Length -eq 0) -or (-not (Test-OnSystemDrive $Path))) {
        return
    }
    try {
        $file = New-Object System.IO.FileInfo -ArgumentList $Path
        $attr = [int]$file.Attributes
        if ($attr -eq 0) {
            # Could not be read (see Resolve-PhysicalPath).
            $Item.Skipped++
            return
        }
        if (($attr -eq -1) -or (($attr -band $directoryBit) -ne 0) -or (($attr -band $notLocalMask) -ne 0)) {
            return
        }
        $Item.Bytes += [long]$file.Length
        $Item.Found = $true
    }
    catch {
        $Item.Skipped++
    }
}

# $true for the errors of reading a folder (no permission, gone, path too
# long, a name .NET cannot handle): the folder is skipped and counted. Anything
# else (for example a method PowerShell cannot call) is a bug and is thrown, so
# that it shows up instead of every folder quietly counting as empty.
function Test-ReadError {
    param($Exception)
    $e = $Exception
    while (($e -is [System.Management.Automation.MethodInvocationException]) -and ($null -ne $e.InnerException)) {
        $e = $e.InnerException
    }
    return (($e -is [System.UnauthorizedAccessException]) -or ($e -is [System.IO.IOException]) -or
        ($e -is [System.Security.SecurityException]) -or ($e -is [System.ArgumentException]) -or
        ($e -is [System.NotSupportedException]))
}

# Closes the find handle behind a directory enumerator.
function Close-Enumerator {
    param($Enumerator)
    try {
        $Enumerator.Dispose()
    }
    catch {
        $Enumerator = $null
    }
}

# Reads the folders of one item until they are all read or the stopwatch
# reaches $TurnEnd (ms). The directory being read when the turn ends keeps its
# enumerator in the item and is continued in the next turn. Every turn reads
# at least one directory, or 512 entries of it, even when it starts late. The
# loop body runs once per file, so it is kept small: no pipeline, no function
# call except for folders with a reparse point.
function Invoke-Walk {
    param([hashtable]$Item, [long]$TurnEnd)
    $clock = $watch
    $skip = $roots
    $mask = $notLocalMask
    $stack = $Item.Stack
    $en = $Item.Enumerator
    $bytes = $Item.Bytes
    $count = $Item.Entries
    $paused = $false
    $started = ($null -ne $en)
    while (-not $paused) {
        if ($null -eq $en) {
            if ($stack.Count -eq 0) {
                $Item.Done = $true
                break
            }
            if ($started -and ($clock.ElapsedMilliseconds -ge $TurnEnd)) {
                break
            }
            $started = $true
            $dir = $stack.Pop()
            try {
                # The enumerator the foreach statement would use. On .NET
                # Framework its type is internal; asking PowerShell for it
                # avoids calling GetEnumerator() on that type directly.
                $en = [System.Management.Automation.LanguagePrimitives]::GetEnumerator($dir.EnumerateFileSystemInfos())
            }
            catch {
                if (-not (Test-ReadError $_.Exception)) {
                    throw
                }
                $Item.Skipped++
                $en = $null
                continue
            }
        }
        try {
            while ($en.MoveNext()) {
                $entry = $en.Current
                $attr = [int]$entry.Attributes
                if (($attr -band 0x10) -ne 0) {
                    if (($attr -band $mask) -eq 0) {
                        if ((($attr -band 0x400) -ne 0) -and (Test-LinkDirectory $entry)) {
                            $Item.Links++
                        }
                        elseif (-not $skip.Contains($entry.FullName)) {
                            $stack.Push($entry)
                        }
                    }
                }
                elseif (($attr -band $mask) -eq 0) {
                    $bytes += $entry.Length
                }
                $count++
                if ((($count % 512) -eq 0) -and ($clock.ElapsedMilliseconds -ge $TurnEnd)) {
                    $paused = $true
                    break
                }
            }
            if (-not $paused) {
                Close-Enumerator $en
                $en = $null
            }
        }
        catch {
            # The directory became unreadable half-way; what was counted stays.
            if (-not (Test-ReadError $_.Exception)) {
                throw
            }
            $Item.Skipped++
            Close-Enumerator $en
            $en = $null
        }
    }
    $Item.Enumerator = $en
    $Item.Bytes = $bytes
    $Item.Entries = $count
}

# "12.3 GB", "850 MB", "40 KB" (binary units, as Explorer shows them).
function Format-Size {
    param([double]$Bytes)
    if ($Bytes -ge 1TB) {
        return ([math]::Round($Bytes / 1TB, 1)).ToString('0.#', $invariant) + ' TB'
    }
    if ($Bytes -ge 1GB) {
        return ([math]::Round($Bytes / 1GB, 1)).ToString('0.#', $invariant) + ' GB'
    }
    if ($Bytes -ge 1MB) {
        return ([math]::Round($Bytes / 1MB)).ToString('0', $invariant) + ' MB'
    }
    return ([math]::Max([double]1, [math]::Round($Bytes / 1KB))).ToString('0', $invariant) + ' KB'
}

function Get-Gigabytes {
    param([double]$Bytes)
    return [math]::Round($Bytes / 1GB, 1)
}

function New-ValueRow {
    param([string]$Id, $Value)
    return [ordered]@{ id = $Id; value = $Value }
}

function New-CodeRow {
    param([string]$Id, [string]$Code)
    return [ordered]@{ id = $Id; code = $Code }
}

try {
    # ---- the system drive
    $systemDrive = ([string]$env:SystemDrive).Trim()
    if ($systemDrive.Length -eq 0) {
        throw 'The SystemDrive environment variable is not set'
    }
    $systemRoot = $systemDrive.TrimEnd($separators) + [System.IO.Path]::DirectorySeparatorChar
    $windowsDir = Resolve-FullPath ([string]$env:SystemRoot)
    if ($windowsDir.Length -eq 0) {
        $windowsDir = Resolve-FullPath ([Environment]::GetFolderPath('Windows'))
    }
    if ($windowsDir.Length -eq 0) {
        throw 'The Windows folder was not found (SystemRoot is not set)'
    }

    $size = [double]0
    $free = [double]0
    $letter = ''
    try {
        $disk = @(Get-CimInstance -ClassName Win32_LogicalDisk -Filter ("DeviceID='{0}'" -f $systemDrive)) | Select-Object -First 1
        if ($null -ne $disk) {
            $size = [double]$disk.Size
            $free = [double]$disk.FreeSpace
            $letter = [string]$disk.DeviceID
        }
    }
    catch {
        $size = [double]0
    }
    if ($size -le 0) {
        $drive = New-Object System.IO.DriveInfo -ArgumentList $systemRoot
        $size = [double]$drive.TotalSize
        $free = [double]$drive.TotalFreeSpace
        $letter = $systemDrive
    }
    if ($size -le 0) {
        throw ('The system drive {0} reports no size' -f $systemDrive)
    }
    if ($letter -notmatch '^[A-Za-z]:$') {
        $letter = ''
    }
    $used = [math]::Max([double]0, $size - $free)

    # ---- %NAME% values and the user's profile folder
    $expandVars['SystemDrive'] = $systemDrive
    $expandVars['SystemRoot'] = $windowsDir
    $expandVars['windir'] = $windowsDir
    foreach ($name in @('ProgramData', 'ALLUSERSPROFILE', 'PUBLIC', 'ProgramFiles', 'ProgramFiles(x86)', 'ProgramW6432', 'CommonProgramFiles')) {
        $value = [Environment]::GetEnvironmentVariable($name)
        if (-not [string]::IsNullOrEmpty($value)) {
            $expandVars[$name] = $value
        }
    }

    $hive = $UserHive.TrimEnd('\')
    if ($hive -match '^(?i)Registry::HKEY_USERS\\(S-1-[0-9]+(-[0-9]+)+)$') {
        $sid = $Matches[1]
        $raw = Get-RegistryText -Path ('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $sid) -Name 'ProfileImagePath'
        $profilePath = Resolve-FullPath (Expand-PathText $raw)
    }
    elseif ($hive -eq 'HKCU:') {
        $profilePath = Resolve-FullPath ([string]$env:USERPROFILE)
    }
    if ($profilePath.Length -gt 0) {
        Add-RegistryVariable -Path ($hive + '\Environment')
        Add-RegistryVariable -Path ($hive + '\Volatile Environment')
        $expandVars['USERPROFILE'] = $profilePath
    }

    # ---- what to measure (system folders first: a folder that two entries
    # name, such as a TEMP set to %SystemRoot%\Temp, goes to the first)
    $item = New-UsageItem -Id 'hiberfil' -Section 'system'
    Add-File -Item $item -Path (Join-Folder -Base $systemRoot -Names @('hiberfil.sys'))

    $item = New-UsageItem -Id 'pagefile' -Section 'system'
    Add-File -Item $item -Path (Join-Folder -Base $systemRoot -Names @('pagefile.sys'))
    Add-File -Item $item -Path (Join-Folder -Base $systemRoot -Names @('swapfile.sys'))

    # System folders are -Optional: one that is missing or cannot be looked
    # at is not shown.
    $item = New-UsageItem -Id 'windows_temp' -Section 'system'
    Add-Folder -Item $item -Path (Join-Folder -Base $windowsDir -Names @('Temp')) -Optional

    $item = New-UsageItem -Id 'update_cache' -Section 'system'
    Add-Folder -Item $item -Path (Join-Folder -Base $windowsDir -Names @('SoftwareDistribution', 'Download')) -Optional

    $item = New-UsageItem -Id 'delivery_optimization' -Section 'system'
    Add-Folder -Item $item -Path (Join-Folder -Base $windowsDir -Names @('ServiceProfiles', 'NetworkService', 'AppData', 'Local', 'Microsoft', 'Windows', 'DeliveryOptimization', 'Cache')) -Optional

    $crashKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\CrashControl'
    $dumpFile = Join-Folder -Base $windowsDir -Names @('MEMORY.DMP')
    $raw = Get-RegistryText -Path $crashKey -Name 'DumpFile'
    if ($raw.Length -gt 0) {
        $dumpFile = Resolve-FullPath (Expand-PathText $raw)
    }
    $minidumpDir = Join-Folder -Base $windowsDir -Names @('Minidump')
    $raw = Get-RegistryText -Path $crashKey -Name 'MinidumpDir'
    if ($raw.Length -gt 0) {
        $minidumpDir = Resolve-FullPath (Expand-PathText $raw)
    }
    $item = New-UsageItem -Id 'memory_dump' -Section 'system'
    Add-File -Item $item -Path $dumpFile
    Add-Folder -Item $item -Path $minidumpDir -Optional

    $item = New-UsageItem -Id 'recycle_bin' -Section 'system'
    Add-Folder -Item $item -Path (Join-Folder -Base $systemRoot -Names @('$Recycle.Bin')) -Optional

    $item = New-UsageItem -Id 'windows_old' -Section 'system'
    Add-Folder -Item $item -Path (Join-Folder -Base $systemRoot -Names @('Windows.old')) -Optional

    if ($profilePath.Length -gt 0) {
        $item = New-UsageItem -Id 'user_temp' -Section 'system'
        $raw = Get-RegistryText -Path ($hive + '\Environment') -Name 'TEMP'
        if ($raw.Length -gt 0) {
            Add-Folder -Item $item -Path (Resolve-FullPath (Expand-PathText $raw))
        }
        else {
            Add-Folder -Item $item -Path (Join-Folder -Base $profilePath -Names @('AppData', 'Local', 'Temp'))
        }

        # Known folders: value name in User Shell Folders, default folder name.
        $shellKey = $hive + '\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'
        $known = @(
            @('desktop', 'Desktop', 'Desktop'),
            @('documents', 'Personal', 'Documents'),
            @('downloads', '{374DE290-123F-4565-9164-39C4925E467B}', 'Downloads'),
            @('pictures', 'My Pictures', 'Pictures'),
            @('videos', 'My Video', 'Videos')
        )
        $knownPaths = @{}
        foreach ($k in $known) {
            $raw = Get-RegistryText -Path $shellKey -Name $k[1]
            if ($raw.Length -gt 0) {
                $knownPaths[$k[0]] = Resolve-FullPath (Expand-PathText $raw)
            }
            else {
                $knownPaths[$k[0]] = Join-Folder -Base $profilePath -Names @($k[2])
            }
        }

        # Chat folders under Documents (where User Shell Folders has it, and
        # the default place), registered before Documents itself so that they
        # are not counted twice.
        $documentFolders = New-Object System.Collections.Generic.List[string]
        foreach ($d in @($knownPaths['documents'], (Join-Folder -Base $profilePath -Names @('Documents')))) {
            if (($d.Length -gt 0) -and (-not $documentFolders.Contains($d))) {
                $documentFolders.Add($d)
            }
        }
        foreach ($chat in @(@('wechat4', 'xwechat_files'), @('wechat3', 'WeChat Files'), @('qq', 'Tencent Files'))) {
            $item = New-UsageItem -Id $chat[0] -Section 'chat'
            foreach ($d in $documentFolders) {
                Add-Folder -Item $item -Path (Join-Folder -Base $d -Names @($chat[1])) -Optional
            }
        }

        foreach ($k in $known) {
            $item = New-UsageItem -Id $k[0] -Section 'user'
            Add-Folder -Item $item -Path $knownPaths[$k[0]]
        }
    }

    # ---- count, taking turns
    $queue = New-Object 'System.Collections.Generic.Queue[object]'
    foreach ($i in $items) {
        if ($i.Stack.Count -gt 0) {
            $queue.Enqueue($i)
        }
        else {
            $i.Done = $true
        }
    }
    while ($queue.Count -gt 0) {
        $now = $watch.ElapsedMilliseconds
        $left = [long]$BudgetMs - $now
        if ($left -le 0) {
            break
        }
        $i = $queue.Dequeue()
        $turn = [long][math]::Floor($left / ($queue.Count + 1))
        $turn = [math]::Min([math]::Min([long]$turnMs, $turn), [long]($itemCapMs - $i.SpentMs))
        $turn = [math]::Max([long]$minTurnMs, $turn)
        Invoke-Walk -Item $i -TurnEnd ([math]::Min([long]$BudgetMs, $now + $turn))
        $i.SpentMs += $watch.ElapsedMilliseconds - $now
        if ((-not $i.Done) -and ($i.SpentMs -lt $itemCapMs)) {
            $queue.Enqueue($i)
        }
    }

    # ---- tables
    $byId = @{}
    $unfinished = 0
    $skipped = 0
    $entries = [long]0
    $listed = [double]0
    foreach ($i in $items) {
        $byId[$i.Id] = $i
        if ($i.Found -and (-not $i.Done)) {
            $unfinished++
        }
        $skipped += $i.Skipped
        $entries += $i.Entries
        $listed += $i.Bytes
    }

    $sections = New-Object System.Collections.Generic.List[object]

    $rows = New-Object System.Collections.Generic.List[object]
    $rows.Add((New-ValueRow -Id 'total' -Value (Format-Size $size)))
    $rows.Add((New-ValueRow -Id 'used' -Value (Format-Size $used)))
    $rows.Add((New-ValueRow -Id 'free' -Value (Format-Size $free)))
    if ($unfinished -gt 0) {
        $rows.Add((New-CodeRow -Id 'note' -Code 'unfinished'))
    }
    if ($profilePath.Length -eq 0) {
        $rows.Add((New-CodeRow -Id 'note' -Code 'no-profile'))
    }
    $driveSection = [ordered]@{ id = 'drive' }
    if ($letter.Length -gt 0) {
        $driveSection['name'] = $letter.ToUpperInvariant()
    }
    $driveSection['rows'] = $rows.ToArray()
    $sections.Add($driveSection)

    $tables = New-Object System.Collections.Generic.List[object]
    foreach ($section in @('system', 'user', 'chat')) {
        $sized = New-Object System.Collections.Generic.List[object]
        $other = New-Object System.Collections.Generic.List[object]
        $total = [double]0
        foreach ($i in $items) {
            if ($i.Section -ne $section) {
                continue
            }
            $total += $i.Bytes
            if ($i.Bytes -gt 0) {
                $id = $i.Id
                if ($i.Found -and (-not $i.Done)) {
                    $id = $i.Id + '_partial'
                }
                $sized.Add([pscustomobject]@{ Bytes = [double]$i.Bytes; Row = (New-ValueRow -Id $id -Value (Format-Size $i.Bytes)) })
            }
            elseif ($i.Found -and (-not $i.Done)) {
                $other.Add((New-CodeRow -Id $i.Id -Code 'not-counted'))
            }
            elseif ($i.Elsewhere -and (-not $i.Found)) {
                $other.Add((New-CodeRow -Id $i.Id -Code 'elsewhere'))
            }
            elseif ($i.Unknown -and (-not $i.Found)) {
                $other.Add((New-CodeRow -Id $i.Id -Code 'unknown'))
            }
        }
        $rows = New-Object System.Collections.Generic.List[object]
        foreach ($s in @($sized.ToArray() | Sort-Object -Property Bytes -Descending)) {
            $rows.Add($s.Row)
        }
        foreach ($r in $other) {
            $rows.Add($r)
        }
        if (($section -eq 'chat') -and ($byId['wechat3'].Bytes -gt 0) -and ($byId['wechat4'].Bytes -gt 0)) {
            $rows.Add((New-CodeRow -Id 'note' -Code 'wechat-both'))
        }
        if ($rows.Count -gt 0) {
            $tables.Add([pscustomobject]@{ Bytes = $total; Section = [ordered]@{ id = $section; rows = $rows.ToArray() } })
        }
    }
    foreach ($t in @($tables.ToArray() | Sort-Object -Property Bytes -Descending)) {
        $sections.Add($t.Section)
    }

    # ---- the biggest group decides the result; the other big ones get advice
    $groups = @(
        @('cleanup', 'cleanup', @('user_temp', 'windows_temp', 'update_cache', 'delivery_optimization', 'memory_dump')),
        @('recycle_bin', 'recycle-bin', @('recycle_bin')),
        @('hiberfil', 'hibernation', @('hiberfil')),
        @('windows_old', 'windows-old', @('windows_old')),
        @('chat', 'chat', @('wechat4', 'wechat3', 'qq')),
        @('user_files', 'user-files', @('desktop', 'documents', 'downloads', 'pictures', 'videos'))
    )
    $ranked = New-Object System.Collections.Generic.List[object]
    for ($g = 0; $g -lt $groups.Count; $g++) {
        $sum = [double]0
        foreach ($member in $groups[$g][2]) {
            if ($byId.ContainsKey($member)) {
                $sum += $byId[$member].Bytes
            }
        }
        $ranked.Add([pscustomobject]@{ Row = $groups[$g][0]; Code = $groups[$g][1]; Bytes = $sum; Order = $g })
    }
    $ranked = @($ranked.ToArray() | Sort-Object -Property @{ Expression = 'Bytes'; Descending = $true }, @{ Expression = 'Order'; Descending = $false })

    $result = 'ok'
    $topGb = [double]0
    if ($ranked[0].Bytes -ge $bigGroupBytes) {
        $result = 'top-' + $ranked[0].Code
        $topGb = Get-Gigabytes $ranked[0].Bytes
        $rows = New-Object System.Collections.Generic.List[object]
        for ($g = 1; $g -lt $ranked.Count; $g++) {
            if ($ranked[$g].Bytes -ge $bigGroupBytes) {
                $rows.Add((New-CodeRow -Id $ranked[$g].Row -Code ('advice-' + $ranked[$g].Code)))
            }
        }
        if ($rows.Count -gt 0) {
            $sections.Add([ordered]@{ id = 'advice'; rows = $rows.ToArray() })
        }
    }

    $driveLetter = ''
    if ($letter.Length -gt 0) {
        $driveLetter = $letter.Substring(0, 1).ToUpperInvariant()
    }
    $output = [pscustomobject]@{
        result   = $result
        facts    = [ordered]@{
            drive      = $driveLetter
            total_gb   = Get-Gigabytes $size
            used_gb    = Get-Gigabytes $used
            free_gb    = Get-Gigabytes $free
            free_pct   = [math]::Round(100 * $free / $size, 1)
            top_gb     = $topGb
            listed_gb  = Get-Gigabytes $listed
            unfinished = $unfinished
            skipped    = $skipped
            entries    = $entries
            seconds    = [math]::Round($watch.Elapsed.TotalSeconds, 1)
        }
        sections = $sections.ToArray()
    }
}
catch {
    throw (Protect-Message $_.Exception.Message)
}
finally {
    # Directories left half-read when the time was up.
    foreach ($i in $items) {
        if ($null -ne $i.Enumerator) {
            Close-Enumerator $i.Enumerator
            $i.Enumerator = $null
        }
    }
}

$output
