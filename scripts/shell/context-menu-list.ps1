# Context menu (right-click menu) entries that programs added. Read-only.
# Where Windows looks: keys under Software\Classes, both in HKLM (all users)
# and in the logged-in user's hive (-UserHive):
#   *                     all files
#   AllFilesystemObjects  files and folders
#   Directory             folders
#   Folder                folders, including virtual ones (This PC, libraries)
#   Directory\Background  the empty space in a folder window
#   DesktopBackground     the empty space on the desktop
#   Drive                 drives
# Three kinds of entries:
#   verb      <scope>\shell\<verb>: a command line (the command subkey, or the
#             first command of its submenu), or a COM class (DelegateExecute,
#             ExplorerCommandHandler). Text: MUIVerb or the default value
#             ("@file,-id" strings are resolved by the engine). extended: the
#             Extended value (only shown with Shift+right-click).
#   handler   <scope>\shellex\ContextMenuHandlers\<name>: a shell extension;
#             its CLSID is the default value (or the key name), its file is
#             InprocServer32 (or LocalServer32) of that class, the user's
#             classes first. .NET extensions registered through mscoree.dll
#             are attributed to their CodeBase.
#   packaged  entries of the Windows 11 menu from packaged apps (and from
#             ordinary programs with a sparse package): AppxManifest.xml,
#             extension windows.fileExplorerContextMenus (ItemType Type, Verb
#             Id and Clsid); the file is the windows.comServer class with that
#             Id. text is the package's DisplayName (maybe an ms-resource
#             string: the engine resolves it with the package full name).
# Whether an entry is switched off is read by the engine itself, from the
# values it writes (ProgrammaticAccessOnly / LegacyDisable of a verb, and
# Shell Extensions\Blocked for a CLSID).
# Output: result = 'ok', items = one object per registration: kind, hive
# (machine / user), scope, key (verb or handler key name; packaged: the Verb
# Id), clsid, text, class_found (handler: the CLSID is registered), package
# (full name), package_name, publisher (package PublisherDisplayName),
# package_system (SignatureKind System), subcommands, extended, in_windows (the
# path is under the Windows folder, even when the file is gone), and what the
# file says about itself (program-info): path, exists, description, company,
# signature, signer, system.
# Paths stay on this PC: the engine shows them in the list, never in the
# report.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$signatureBudgetMs = 20000
$maxItems = 400
$scopes = @('*', 'AllFilesystemObjects', 'Directory', 'Folder', 'Directory\Background', 'DesktopBackground', 'Drive')
$guidPattern = '^\{?([0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12})\}?$'
$noExpand = [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames

# ---- shared block program-info: identical in startup/list.ps1, shell/context-menu-list.ps1 and tools/system/scheduled-tasks.ps1 (medkit-data check compares them) ----
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

$items = New-Object System.Collections.Generic.List[object]
$factsCache = @{}

# The logged-in user's SID from -UserHive ('' when it is HKCU:).
$userSid = ''
if ($UserHive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
    $userSid = $Matches[1]
}
$machineClasses = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SOFTWARE\Classes')
if ($userSid.Length -gt 0) {
    $userClasses = [Microsoft.Win32.Registry]::Users.OpenSubKey($userSid + '\Software\Classes')
}
else {
    $userClasses = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Classes')
}

# {XXXXXXXX-...} in upper case, or '' when the text is not a GUID.
function Get-Clsid {
    param([string]$Text)
    $m = [regex]::Match($Text.Trim(), $guidPattern)
    if (-not $m.Success) {
        return ''
    }
    return '{' + $m.Groups[1].Value.ToUpperInvariant() + '}'
}

function Get-CachedFacts {
    param([string]$Path, [bool]$Hosted)
    $cacheKey = ([string]$Hosted) + '|' + $Path.ToLowerInvariant()
    if (-not $factsCache.ContainsKey($cacheKey)) {
        $factsCache[$cacheKey] = Get-FileFacts -Path $Path -Hosted $Hosted
    }
    return $factsCache[$cacheKey]
}

# A value as text; -Unexpanded keeps %variables% (command lines: Get-Program
# expands them for the logged-in user).
function Get-KeyText {
    param($Key, [string]$Name, [bool]$Unexpanded = $false)
    if ($null -eq $Key) {
        return ''
    }
    if ($Unexpanded) {
        return Get-Text $Key.GetValue($Name, '', $noExpand)
    }
    return Get-Text $Key.GetValue($Name, '')
}

# The file of a COM class: Found (the class is registered), Name (its default
# value), Program (Path and Hosted, as Get-Program returns).
function Get-ClassServer {
    param([string]$Clsid)
    foreach ($root in @($userClasses, $machineClasses)) {
        if ($null -eq $root) {
            continue
        }
        $class = $root.OpenSubKey('CLSID\' + $Clsid)
        if ($null -eq $class) {
            continue
        }
        try {
            $name = Get-KeyText $class ''
            foreach ($server in @('InprocServer32', 'LocalServer32')) {
                $sub = $class.OpenSubKey($server)
                if ($null -eq $sub) {
                    continue
                }
                try {
                    $line = Get-KeyText $sub '' $true
                    if ($line.Length -eq 0) {
                        continue
                    }
                    $program = Get-Program $line
                    # .NET shell extensions load through mscoree.dll; the assembly is CodeBase
                    if ((Get-Leaf $program.Path).ToLowerInvariant() -eq 'mscoree.dll') {
                        $codeBase = Get-KeyText $sub 'CodeBase'
                        $m = [regex]::Match($codeBase, '(?i)^file:///(.+)$')
                        if ($m.Success) {
                            $program = [pscustomobject]@{ Path = $m.Groups[1].Value.Replace('/', '\'); Hosted = $true }
                        }
                        else {
                            $program = [pscustomobject]@{ Path = $program.Path; Hosted = $true }
                        }
                    }
                    return [pscustomobject]@{ Found = $true; Name = $name; Program = $program }
                }
                finally {
                    $sub.Close()
                }
            }
            return [pscustomobject]@{ Found = $true; Name = $name; Program = [pscustomobject]@{ Path = ''; Hosted = $false } }
        }
        finally {
            $class.Close()
        }
    }
    return [pscustomobject]@{ Found = $false; Name = ''; Program = [pscustomobject]@{ Path = ''; Hosted = $false } }
}

# One registration: the defaults, what the file says about itself, then
# -Fields (which win: a packaged entry sets its own signature).
function Add-Item {
    param([hashtable]$Fields, $Program)
    $info = [ordered]@{
        kind           = $Fields.kind
        hive           = $Fields.hive
        scope          = $Fields.scope
        key            = $Fields.key
        clsid          = ''
        text           = ''
        class_found    = $true
        package        = ''
        package_name   = ''
        publisher      = ''
        package_system = $false
        subcommands    = $false
        extended       = $false
        path           = $Program.Path
        in_windows     = $Program.Path.StartsWith($windowsDir + '\', [System.StringComparison]::OrdinalIgnoreCase)
    }
    $facts = Get-CachedFacts -Path $Program.Path -Hosted $Program.Hosted
    foreach ($name in @($facts.Keys)) {
        $info[$name] = $facts[$name]
    }
    foreach ($name in @($Fields.Keys)) {
        $info[$name] = $Fields[$name]
    }
    $items.Add($info)
}

# The command of a verb key: its command subkey, else the first command of
# its submenu (<verb>\shell\<sub>\command).
function Get-VerbCommand {
    param($Key)
    $command = $Key.OpenSubKey('command')
    if ($null -ne $command) {
        try {
            return [pscustomobject]@{ Line = (Get-KeyText $command '' $true); Delegate = (Get-KeyText $command 'DelegateExecute') }
        }
        finally {
            $command.Close()
        }
    }
    $sub = $Key.OpenSubKey('shell')
    if ($null -ne $sub) {
        try {
            foreach ($name in @($sub.GetSubKeyNames())) {
                $child = $sub.OpenSubKey($name + '\command')
                if ($null -ne $child) {
                    try {
                        $line = Get-KeyText $child '' $true
                        if ($line.Length -gt 0) {
                            return [pscustomobject]@{ Line = $line; Delegate = '' }
                        }
                    }
                    finally {
                        $child.Close()
                    }
                }
            }
        }
        finally {
            $sub.Close()
        }
    }
    return [pscustomobject]@{ Line = ''; Delegate = '' }
}

function Add-Verbs {
    param($Classes, [string]$Hive, [string]$Scope)
    $shell = $Classes.OpenSubKey($Scope + '\shell')
    if ($null -eq $shell) {
        return
    }
    try {
        foreach ($verb in @($shell.GetSubKeyNames())) {
            if ($items.Count -ge $maxItems) {
                return
            }
            $key = $shell.OpenSubKey($verb)
            if ($null -eq $key) {
                continue
            }
            try {
                $names = @($key.GetValueNames())
                $text = Get-KeyText $key 'MUIVerb'
                if ($text.Length -eq 0) {
                    $text = Get-KeyText $key ''
                }
                $command = Get-VerbCommand $key
                $program = [pscustomobject]@{ Path = ''; Hosted = $false }
                $clsid = ''
                if ($command.Line.Length -gt 0) {
                    $program = Get-Program $command.Line
                }
                else {
                    $clsid = Get-Clsid $command.Delegate
                    if ($clsid.Length -eq 0) {
                        $clsid = Get-Clsid (Get-KeyText $key 'ExplorerCommandHandler')
                    }
                    if ($clsid.Length -gt 0) {
                        $program = (Get-ClassServer $clsid).Program
                    }
                }
                $hasShell = $false
                $sub = $key.OpenSubKey('shell')
                if ($null -ne $sub) {
                    $hasShell = $true
                    $sub.Close()
                }
                Add-Item -Program $program -Fields @{
                    kind        = 'verb'
                    hive        = $Hive
                    scope       = $Scope
                    key         = $verb
                    clsid       = $clsid
                    text        = $text
                    subcommands = ($hasShell -or ($names -contains 'SubCommands') -or ($names -contains 'ExtendedSubCommandsKey'))
                    extended    = ($names -contains 'Extended')
                }
            }
            finally {
                $key.Close()
            }
        }
    }
    finally {
        $shell.Close()
    }
}

function Add-Handlers {
    param($Classes, [string]$Hive, [string]$Scope)
    $handlers = $Classes.OpenSubKey($Scope + '\shellex\ContextMenuHandlers')
    if ($null -eq $handlers) {
        return
    }
    try {
        foreach ($name in @($handlers.GetSubKeyNames())) {
            if ($items.Count -ge $maxItems) {
                return
            }
            $key = $handlers.OpenSubKey($name)
            if ($null -eq $key) {
                continue
            }
            try {
                $clsid = Get-Clsid (Get-KeyText $key '')
            }
            finally {
                $key.Close()
            }
            if ($clsid.Length -eq 0) {
                $clsid = Get-Clsid $name
            }
            if ($clsid.Length -eq 0) {
                continue
            }
            $server = Get-ClassServer $clsid
            Add-Item -Program $server.Program -Fields @{
                kind        = 'handler'
                hive        = $Hive
                scope       = $Scope
                key         = $name
                clsid       = $clsid
                text        = $server.Name
                class_found = $server.Found
            }
        }
    }
    finally {
        $handlers.Close()
    }
}

function Read-Manifest {
    param([string]$Text)
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [System.Xml.XmlReader]::Create((New-Object System.IO.StringReader $Text), $settings)
    try {
        $doc = New-Object System.Xml.XmlDocument
        $doc.Load($reader)
        return $doc
    }
    finally {
        $reader.Close()
    }
}

function Add-Packaged {
    try {
        if ($userSid.Length -gt 0) {
            $packages = @(Get-AppxPackage -User $userSid -ErrorAction Stop)
        }
        else {
            $packages = @(Get-AppxPackage -ErrorAction Stop)
        }
    }
    catch {
        Write-Verbose ('Get-AppxPackage failed: {0}' -f $_.Exception.Message)
        return
    }
    foreach ($package in $packages) {
        if (($items.Count -ge $maxItems) -or $package.IsFramework -or $package.IsResourcePackage) {
            continue
        }
        $location = Get-Text $package.InstallLocation
        if ($location.Length -eq 0) {
            continue
        }
        try {
            $text = [System.IO.File]::ReadAllText($location + '\AppxManifest.xml')
        }
        catch {
            continue
        }
        if ($text.IndexOf('windows.fileExplorerContextMenus', [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
            continue
        }
        try {
            $doc = Read-Manifest $text
        }
        catch {
            Write-Verbose ('{0}: manifest not readable' -f $package.Name)
            continue
        }
        $classPaths = @{}
        foreach ($class in @($doc.SelectNodes("//*[local-name()='Class'][@Id]"))) {
            $id = Get-Clsid ([string]$class.GetAttribute('Id'))
            $path = Get-Text $class.GetAttribute('Path')
            if (($id.Length -gt 0) -and ($path.Length -gt 0)) {
                $classPaths[$id] = $path
            }
        }
        $displayName = ''
        $node = $doc.SelectSingleNode("/*[local-name()='Package']/*[local-name()='Properties']/*[local-name()='DisplayName']")
        if ($null -ne $node) {
            $displayName = Get-Text $node.InnerText
        }
        $publisher = ''
        $node = $doc.SelectSingleNode("/*[local-name()='Package']/*[local-name()='Properties']/*[local-name()='PublisherDisplayName']")
        if ($null -ne $node) {
            $publisher = Get-Text $node.InnerText
        }
        $verbs = @($doc.SelectNodes("//*[local-name()='Extension'][@Category='windows.fileExplorerContextMenus']//*[local-name()='Verb']"))
        foreach ($verb in $verbs) {
            $clsid = Get-Clsid ([string]$verb.GetAttribute('Clsid'))
            if ($clsid.Length -eq 0) {
                continue
            }
            # The DLL when it is where the manifest says; else the package folder (a
            # sparse package's files can live elsewhere). Either way the package is
            # installed, so the entry exists. Files inside a package are not signed
            # one by one: Windows checked the package's signature when installing it
            # (unsigned only for a package registered in developer mode).
            $path = $location
            if ($classPaths.ContainsKey($clsid)) {
                $dll = $location + '\' + $classPaths[$clsid].TrimStart('\')
                if (Test-Path -LiteralPath $dll -PathType Leaf) {
                    $path = $dll
                }
            }
            $program = [pscustomobject]@{ Path = $path; Hosted = $false }
            $signature = 'valid'
            if ($package.IsDevelopmentMode) {
                $signature = 'unsigned'
            }
            $type = ''
            if ($verb.ParentNode -is [System.Xml.XmlElement]) {
                $type = Get-Text $verb.ParentNode.GetAttribute('Type')
            }
            Add-Item -Program $program -Fields @{
                kind           = 'packaged'
                hive           = 'user'
                scope          = $type
                key            = (Get-Text $verb.GetAttribute('Id'))
                clsid          = $clsid
                text           = $displayName
                package        = (Get-Text $package.PackageFullName)
                package_name   = (Get-Text $package.Name)
                publisher      = $publisher
                package_system = ([string]$package.SignatureKind -eq 'System')
                exists         = $true
                signature      = $signature
                signer         = $publisher
            }
        }
    }
}

foreach ($scope in $scopes) {
    if ($null -ne $machineClasses) {
        Add-Verbs -Classes $machineClasses -Hive 'machine' -Scope $scope
        Add-Handlers -Classes $machineClasses -Hive 'machine' -Scope $scope
    }
    if ($null -ne $userClasses) {
        Add-Verbs -Classes $userClasses -Hive 'user' -Scope $scope
        Add-Handlers -Classes $userClasses -Hive 'user' -Scope $scope
    }
}
Add-Packaged

[pscustomobject]@{
    result = 'ok'
    items  = $items.ToArray()
}
