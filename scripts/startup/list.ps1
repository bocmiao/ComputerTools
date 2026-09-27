# Startup items: the programs that start when the user signs in.
# Read-only. The places Task Manager's "Startup apps" page lists (apart from
# packaged Store apps, which have their own switch in Settings):
#   user-run       <UserHive>\Software\Microsoft\Windows\CurrentVersion\Run
#   machine-run    HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run
#   machine-run32  HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run
#   user-folder    the user's Startup folder (User Shell Folders\Startup)
#   common-folder  the Startup folder of all users (Common Startup)
# Whether an item is switched off (the StartupApproved values, the switch Task
# Manager uses) is read by the engine itself, from the same place it writes.
# For every item: the program it starts (a quoted path, or the text up to the
# first .exe; shortcuts are resolved; for rundll32, wscript and other hosts the
# file they run), whether that file exists, its description and company (what
# Task Manager shows) and its Authenticode signature. Checking a signature
# reads the whole file, so the checks stop after $signatureBudgetMs and the
# rest are reported as skipped.
# Paths stay on this PC: the engine shows them in the list, never in the
# report.
# Output: result = 'ok', items = one object per item: source, name, path,
# exists, description, company, signature (valid / unsigned / invalid /
# unknown / skipped), signer, system (the program is under the Windows folder).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$signatureBudgetMs = 30000
$maxItems = 200
$runKey = 'Software\Microsoft\Windows\CurrentVersion\Run'
$shellFolders = 'Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'
# ---- shared block program-info: identical in startup/list.ps1 and shell/context-menu-list.ps1 (medkit-data check compares them) ----
# Which program a command line starts, and what that file says about itself.
# Uses $UserHive (the logged-in user's hive, or HKCU:) and $signatureBudgetMs.
$windowsDir = [Environment]::GetFolderPath('Windows').TrimEnd('\')
# Paths are joined as strings: Join-Path needs the drive to exist.
$system32 = $windowsDir + '\System32'
# Programs that run another file: the file they run is what the item is about.
$hostPrograms = @('rundll32.exe', 'wscript.exe', 'cscript.exe', 'mshta.exe', 'cmd.exe', 'powershell.exe', 'pwsh.exe', 'conhost.exe')
$fileExtensions = '(exe|com|bat|cmd|dll|vbs|vbe|js|jse|wsf|hta|ps1|lnk|scr|msc)'

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return (([string]$Value) -replace '[\x00-\x1f]', ' ').Trim()
}

# The logged-in user's profile folder: from the SID in -UserHive, or this
# process's own profile when the hive is HKCU:.
function Get-ProfileDir {
    if ($UserHive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
        $key = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $Matches[1]
        try {
            $item = Get-ItemProperty -LiteralPath $key -Name 'ProfileImagePath' -ErrorAction Stop
            return [Environment]::ExpandEnvironmentVariables((Get-Text $item.ProfileImagePath))
        }
        catch {
            return ''
        }
    }
    return Get-Text $env:USERPROFILE
}

$profileDir = Get-ProfileDir

# Expands %USERPROFILE%, %APPDATA% and %LOCALAPPDATA% for the logged-in user
# (not the account the tool runs as), then the rest from this process.
function Expand-UserText {
    param([string]$Text)
    if ($profileDir.Length -gt 0) {
        $pairs = @(
            @('%USERPROFILE%', $profileDir),
            @('%APPDATA%', ($profileDir + '\AppData\Roaming')),
            @('%LOCALAPPDATA%', ($profileDir + '\AppData\Local'))
        )
        foreach ($pair in $pairs) {
            $at = $Text.IndexOf($pair[0], [System.StringComparison]::OrdinalIgnoreCase)
            while ($at -ge 0) {
                $Text = $Text.Substring(0, $at) + $pair[1] + $Text.Substring($at + $pair[0].Length)
                $at = $Text.IndexOf($pair[0], $at + $pair[1].Length, [System.StringComparison]::OrdinalIgnoreCase)
            }
        }
    }
    return [Environment]::ExpandEnvironmentVariables($Text)
}

# The file name at the end of a path (string only: no path parsing, which
# throws on characters Windows does not allow).
function Get-Leaf {
    param([string]$Path)
    return $Path.Substring($Path.LastIndexOf('\') + 1)
}

# The folders of the machine-wide PATH (the system variable: not the user's,
# and not this process's), for bare program names like powershell.exe.
$machinePathDirs = @()
try {
    $envKey = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\CurrentControlSet\Control\Session Manager\Environment')
    if ($null -ne $envKey) {
        try {
            $machinePathText = [string]$envKey.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        }
        finally {
            $envKey.Close()
        }
        $machinePathDirs = @($machinePathText.Split(';') | ForEach-Object { [Environment]::ExpandEnvironmentVariables($_.Trim()).TrimEnd('\') } | Where-Object { $_ -match '^[A-Za-z]:\\' })
    }
}
catch {
    $machinePathDirs = @()
}

# A bare file name (ctfmon.exe) is looked up where Windows would find it.
function Resolve-ProgramPath {
    param([string]$Path)
    $Path = $Path.Trim().Trim('"')
    if (($Path.Length -eq 0) -or $Path.Contains('\')) {
        return $Path
    }
    foreach ($dir in (@($system32, $windowsDir) + $machinePathDirs)) {
        $candidate = $dir + '\' + $Path
        try {
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                return $candidate
            }
        }
        catch {
            return $Path
        }
    }
    return $Path
}

# Splits a command line into the program and the rest.
function Split-Command {
    param([string]$Command)
    $cmd = (Expand-UserText $Command).Trim()
    if ($cmd.StartsWith('"')) {
        $end = $cmd.IndexOf('"', 1)
        if ($end -gt 1) {
            return @($cmd.Substring(1, $end - 1), $cmd.Substring($end + 1).Trim())
        }
        return @($cmd.Trim('"'), '')
    }
    $m = [regex]::Match($cmd, '(?i)^(.+?\.' + $fileExtensions + ')(\s+(.*))?$')
    if ($m.Success) {
        return @($m.Groups[1].Value, $m.Groups[4].Value.Trim())
    }
    $parts = $cmd -split '\s+', 2
    if ($parts.Count -gt 1) {
        return @($parts[0], $parts[1])
    }
    return @($cmd, '')
}

# The file a host program (rundll32, wscript ...) runs: the first argument
# that looks like a file ("x.dll,Entry" for rundll32).
function Get-HostedFile {
    param([string]$Arguments)
    $m = [regex]::Match($Arguments, '(?i)"([^"]+?\.' + $fileExtensions + ')"|([^\s",]+?\.' + $fileExtensions + ')(?=[\s,]|$)')
    if (-not $m.Success) {
        return ''
    }
    if ($m.Groups[1].Success) {
        return $m.Groups[1].Value
    }
    return $m.Groups[3].Value
}

function Get-Program {
    param([string]$Command)
    $parts = Split-Command $Command
    $program = Resolve-ProgramPath $parts[0]
    if ($program.Length -eq 0) {
        return [pscustomobject]@{ Path = ''; Hosted = $false }
    }
    if ($hostPrograms -contains (Get-Leaf $program).ToLowerInvariant()) {
        $hosted = Get-HostedFile $parts[1]
        if ($hosted.Length -gt 0) {
            return [pscustomobject]@{ Path = (Resolve-ProgramPath $hosted); Hosted = $true }
        }
        return [pscustomobject]@{ Path = $program; Hosted = $true }
    }
    return [pscustomobject]@{ Path = $program; Hosted = $false }
}

$watch = [System.Diagnostics.Stopwatch]::StartNew()

function Get-SignerName {
    param([string]$Subject)
    foreach ($field in @('O', 'CN')) {
        $m = [regex]::Match($Subject, '(?:^|,\s*)' + $field + '=("([^"]*)"|[^,]*)')
        if ($m.Success) {
            $value = $m.Groups[2].Value
            if (-not $m.Groups[2].Success) {
                $value = $m.Groups[1].Value
            }
            $value = Get-Text $value
            if ($value.Length -gt 0) {
                return $value
            }
        }
    }
    return ''
}

# What a program file says about itself: exists, description and company
# (what Task Manager shows), signature (valid / unsigned / invalid / unknown /
# skipped) and its signer, and system (the file is under the Windows folder and
# is not run by a host program). Checking a signature reads the whole file, so
# the checks stop after $signatureBudgetMs and the rest are reported as skipped.
function Get-FileFacts {
    param([string]$Path, [bool]$Hosted)
    $info = [ordered]@{
        exists      = $false
        description = ''
        company     = ''
        signature   = 'unknown'
        signer      = ''
        system      = $false
    }
    if ($Path.Length -eq 0) {
        return $info
    }
    try {
        $file = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    }
    catch {
        return $info
    }
    if ($file.PSIsContainer) {
        return $info
    }
    $info.exists = $true
    # A host program (rundll32 running a DLL, wscript running a script) is not
    # "Windows' own" just because rundll32 is.
    $info.system = ((-not $Hosted) -and $Path.StartsWith($windowsDir + '\', [System.StringComparison]::OrdinalIgnoreCase))
    try {
        $version = $file.VersionInfo
        $info.description = Get-Text $version.FileDescription
        if ($info.description.Length -eq 0) {
            $info.description = Get-Text $version.ProductName
        }
        $info.company = Get-Text $version.CompanyName
    }
    catch {
        $info.description = ''
    }
    if ($watch.ElapsedMilliseconds -ge $signatureBudgetMs) {
        $info.signature = 'skipped'
        return $info
    }
    try {
        $sig = Get-AuthenticodeSignature -LiteralPath $Path -ErrorAction Stop
        switch ([string]$sig.Status) {
            'Valid' { $info.signature = 'valid' }
            'NotSigned' { $info.signature = 'unsigned' }
            'HashMismatch' { $info.signature = 'invalid' }
            'NotTrusted' { $info.signature = 'invalid' }
            'Incompatible' { $info.signature = 'invalid' }
            default { $info.signature = 'unknown' }
        }
        if (($info.signature -eq 'valid') -and ($null -ne $sig.SignerCertificate)) {
            $info.signer = Get-SignerName ([string]$sig.SignerCertificate.Subject)
        }
    }
    catch {
        $info.signature = 'unknown'
    }
    return $info
}
# ---- end of shared block program-info ----

function New-ItemInfo {
    param([string]$Source, [string]$Name, [string]$Path, [bool]$Hosted)
    $info = [ordered]@{
        source = $Source
        name   = $Name
        path   = $Path
    }
    $facts = Get-FileFacts -Path $Path -Hosted $Hosted
    foreach ($key in @($facts.Keys)) {
        $info[$key] = $facts[$key]
    }
    return $info
}

$items = New-Object System.Collections.Generic.List[object]

function Add-RunItems {
    param([string]$Source, [string]$KeyPath)
    try {
        $key = Get-Item -LiteralPath $KeyPath -ErrorAction Stop
    }
    catch {
        return
    }
    foreach ($name in @($key.GetValueNames())) {
        if (($items.Count -ge $maxItems) -or ([string]::IsNullOrEmpty($name))) {
            continue
        }
        $kind = $key.GetValueKind($name)
        if (($kind -ne [Microsoft.Win32.RegistryValueKind]::String) -and ($kind -ne [Microsoft.Win32.RegistryValueKind]::ExpandString)) {
            continue
        }
        $command = Get-Text $key.GetValue($name, '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        if ($command.Length -eq 0) {
            continue
        }
        $program = Get-Program $command
        $items.Add((New-ItemInfo -Source $Source -Name $name -Path $program.Path -Hosted $program.Hosted))
    }
}

$shell = $null
function Get-ShortcutTarget {
    param([string]$Path)
    if ($null -eq $script:shell) {
        $script:shell = New-Object -ComObject WScript.Shell
    }
    $link = $script:shell.CreateShortcut($Path)
    $target = Get-Text $link.TargetPath
    if ($target.Length -eq 0) {
        return [pscustomobject]@{ Path = ''; Hosted = $false }
    }
    if ($hostPrograms -contains (Get-Leaf $target).ToLowerInvariant()) {
        $hosted = Get-HostedFile (Expand-UserText (Get-Text $link.Arguments))
        if ($hosted.Length -gt 0) {
            return [pscustomobject]@{ Path = (Resolve-ProgramPath $hosted); Hosted = $true }
        }
        return [pscustomobject]@{ Path = $target; Hosted = $true }
    }
    return [pscustomobject]@{ Path = $target; Hosted = $false }
}

function Add-FolderItems {
    param([string]$Source, [string]$Folder)
    if (($Folder.Length -eq 0) -or (-not (Test-Path -LiteralPath $Folder -PathType Container))) {
        return
    }
    foreach ($file in @(Get-ChildItem -LiteralPath $Folder -File -Force -ErrorAction SilentlyContinue)) {
        if (($items.Count -ge $maxItems) -or ($file.Name -ieq 'desktop.ini')) {
            continue
        }
        $program = [pscustomobject]@{ Path = $file.FullName; Hosted = $false }
        if ($file.Extension -ieq '.lnk') {
            try {
                $program = Get-ShortcutTarget $file.FullName
            }
            catch {
                $program = [pscustomobject]@{ Path = ''; Hosted = $false }
            }
        }
        elseif ($file.Extension -ieq '.url') {
            $program = [pscustomobject]@{ Path = ''; Hosted = $false }
        }
        $items.Add((New-ItemInfo -Source $Source -Name $file.Name -Path $program.Path -Hosted $program.Hosted))
    }
}

# A folder from User Shell Folders (REG_EXPAND_SZ, not expanded yet).
function Get-ShellFolder {
    param([string]$KeyPath, [string]$Name)
    try {
        $key = Get-Item -LiteralPath $KeyPath -ErrorAction Stop
        $raw = Get-Text $key.GetValue($Name, '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    }
    catch {
        return ''
    }
    if ($raw.Length -eq 0) {
        return ''
    }
    return Expand-UserText $raw
}

$userRoot = $UserHive.TrimEnd('\')
Add-RunItems -Source 'user-run' -KeyPath ($userRoot + '\' + $runKey)
Add-RunItems -Source 'machine-run' -KeyPath ('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run')
Add-RunItems -Source 'machine-run32' -KeyPath ('HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run')

$userStartup = Get-ShellFolder -KeyPath ($userRoot + '\' + $shellFolders) -Name 'Startup'
if (($userStartup.Length -eq 0) -and ($profileDir.Length -gt 0)) {
    $userStartup = $profileDir + '\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup'
}
Add-FolderItems -Source 'user-folder' -Folder $userStartup

$commonStartup = Get-ShellFolder -KeyPath ('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders') -Name 'Common Startup'
if ($commonStartup.Length -eq 0) {
    $commonStartup = [Environment]::GetFolderPath('CommonApplicationData') + '\Microsoft\Windows\Start Menu\Programs\StartUp'
}
Add-FolderItems -Source 'common-folder' -Folder $commonStartup

[pscustomobject]@{
    result = 'ok'
    items  = $items.ToArray()
}
