# Tool: system.scheduled-tasks (info)
# The scheduled tasks that did not come with Windows: mostly software
# updaters, but also what pops up ads, starts a program again after it was
# taken out of startup, or wakes the PC at night. Read-only.
# Tasks under \Microsoft\ are Windows' own and left out. For each other task
# (Get-ScheduledTask; its exec actions, see the shared block for how a
# command line is read): the program it runs (file name only; for script
# hosts and rundll32 the file they run), the company in the file, when it
# runs (one row per kind of trigger, at most three), whether it is on, when
# it last ran, and what makes it worth a look:
#   hidden     Settings.Hidden: Task Scheduler does not show it unless "Show
#              Hidden Tasks" is on
#   wake       Settings.WakeToRun: wakes the PC from sleep to run
#   missing    the program is not there (left behind by removed software)
#   script     runs through a script host or rundll32 ($hostPrograms)
#   user-dir   the program is in the user's folder (AppData, Temp) and has
#              no valid digital signature
#   unsigned   the program (elsewhere) has no valid digital signature
# The tasks worth a look come first; at most $maxTasks are listed.
# Privacy: task names can hold the user's SID (Google, Edge and OneDrive add
# it) and user folder names; both are replaced. Of paths only file names are
# kept.
# Result codes: flagged / found / none. Facts: tasks (tasks not from
# Windows), flagged (the ones worth a look).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

$signatureBudgetMs = 20000
$maxTasks = 25
$maxRead = 80

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

$profileLeaf = ''
if ($profileDir.Length -gt 0) {
    $profileLeaf = Get-Leaf $profileDir.TrimEnd('\')
}

# A task name to show: no SID, no user folder name, at most 80 characters.
function Get-TaskLabel {
    param([string]$Path, [string]$Name)
    $text = Get-Text ($Path.TrimStart('\') + $Name)
    $text = $text -replace '(?i)S-1-5-21(-\d+)+', '*'
    $text = $text -replace '(?i)(\\(Users|Documents and Settings)\\)[^\\]+', '$1*'
    if (($profileLeaf.Length -ge 2) -and ($text.IndexOf($profileLeaf, [System.StringComparison]::OrdinalIgnoreCase) -ge 0)) {
        $text = [regex]::Replace($text, [regex]::Escape($profileLeaf), '*', 'IgnoreCase')
    }
    if ($text.Length -gt 80) {
        $text = $text.Substring(0, 80)
    }
    return $text
}

# The kinds of a task's triggers, in a fixed order.
function Get-TriggerKinds {
    param($Task)
    $kinds = New-Object System.Collections.Generic.List[string]
    foreach ($trigger in @($Task.Triggers)) {
        if ($null -eq $trigger) {
            continue
        }
        $class = [string]$trigger.CimClass.CimClassName
        $kind = 'other'
        switch ($class) {
            'MSFT_TaskLogonTrigger' { $kind = 'logon' }
            'MSFT_TaskBootTrigger' { $kind = 'startup' }
            'MSFT_TaskDailyTrigger' { $kind = 'daily' }
            'MSFT_TaskWeeklyTrigger' { $kind = 'weekly' }
            'MSFT_TaskTimeTrigger' { $kind = 'once' }
            'MSFT_TaskIdleTrigger' { $kind = 'idle' }
            'MSFT_TaskEventTrigger' { $kind = 'event' }
            'MSFT_TaskSessionStateChangeTrigger' { $kind = 'session' }
            'MSFT_TaskRegistrationTrigger' { $kind = 'registration' }
        }
        $interval = ''
        if ($null -ne $trigger.Repetition) {
            $interval = [string]$trigger.Repetition.Interval
        }
        if ($interval.Length -gt 0) {
            $kind = 'repeat'
        }
        if (-not $kinds.Contains($kind)) {
            $kinds.Add($kind)
        }
    }
    $order = @('logon', 'startup', 'repeat', 'daily', 'weekly', 'once', 'idle', 'event', 'session', 'registration', 'other')
    return , @($order | Where-Object { $kinds.Contains($_) } | Select-Object -First 3)
}

$tasks = @()
try {
    $tasks = @(Get-ScheduledTask -ErrorAction Stop | Where-Object { -not ([string]$_.TaskPath).StartsWith('\Microsoft\', [System.StringComparison]::OrdinalIgnoreCase) })
}
catch {
    Write-Verbose ('The scheduled tasks could not be listed: ' + $_.Exception.Message)
}

$entries = New-Object System.Collections.Generic.List[object]
foreach ($task in @($tasks | Select-Object -First $maxRead)) {
    $flags = New-Object System.Collections.Generic.List[string]
    if ($task.Settings.Hidden -eq $true) {
        $flags.Add('hidden')
    }
    if ($task.Settings.WakeToRun -eq $true) {
        $flags.Add('wake')
    }
    $programName = ''
    $programCode = 'none'
    $company = ''
    foreach ($action in @($task.Actions)) {
        if ($null -eq $action) {
            continue
        }
        $execute = Get-Text $action.Execute
        if ($execute.Length -eq 0) {
            if ([string]$action.CimClass.CimClassName -eq 'MSFT_TaskComHandlerAction') {
                $programCode = 'com'
            }
            continue
        }
        $command = $execute
        if ((-not $execute.StartsWith('"')) -and $execute.Contains(' ')) {
            $command = '"' + $execute + '"'
        }
        $command = ($command + ' ' + (Get-Text $action.Arguments)).Trim()
        $program = Get-Program $command
        if ($program.Path.Length -eq 0) {
            continue
        }
        $facts = Get-FileFacts $program.Path $program.Hosted
        $programName = Get-Leaf $program.Path
        $programCode = ''
        $company = $facts.company
        if (-not $facts.exists) {
            $flags.Add('missing')
        }
        if ($program.Hosted) {
            $flags.Add('script')
        }
        # Signed programs in the user's folder are common (OneDrive, Teams);
        # unsigned ones there are what adware looks like.
        $expanded = Expand-UserText $program.Path
        $inUserDir = (($profileDir.Length -gt 0) -and $expanded.StartsWith($profileDir + '\', [System.StringComparison]::OrdinalIgnoreCase)) -or ($expanded -match '(?i)\\(Temp|AppData)\\')
        if ($facts.exists -and ($facts.signature -ne 'valid') -and $inUserDir) {
            $flags.Add('user-dir')
        }
        elseif ($facts.exists -and (@('unsigned', 'invalid') -contains $facts.signature)) {
            $flags.Add('unsigned')
        }
        break
    }
    $last = ''
    try {
        $info = Get-ScheduledTaskInfo -InputObject $task -ErrorAction Stop
        if (($null -ne $info.LastRunTime) -and ([DateTime]$info.LastRunTime).Year -ge 2001) {
            $last = ([DateTime]$info.LastRunTime).ToString('yyyy-MM-dd HH:mm', [Globalization.CultureInfo]::InvariantCulture)
        }
    }
    catch {
        $last = ''
    }
    $entries.Add([pscustomobject]@{
            Label   = Get-TaskLabel ([string]$task.TaskPath) ([string]$task.TaskName)
            Program = $programName
            Code    = $programCode
            Company = $company
            Kinds   = Get-TriggerKinds $task
            On      = ([string]$task.State -ne 'Disabled')
            Last    = $last
            Flags   = $flags.ToArray()
        })
}

$flaggedCount = @($entries | Where-Object { $_.Flags.Count -gt 0 }).Count
$shown = @($entries | Sort-Object -Property @{ Expression = { $_.Flags.Count }; Descending = $true }, @{ Expression = { $_.Label }; Descending = $false } | Select-Object -First $maxTasks)
$sections = New-Object System.Collections.Generic.List[object]
foreach ($entry in $shown) {
    $rows = New-Object System.Collections.Generic.List[object]
    if ($entry.Program.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'program'; value = $entry.Program })
    }
    else {
        $rows.Add([ordered]@{ id = 'program'; code = $entry.Code })
    }
    if ($entry.Company.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'company'; value = $entry.Company })
    }
    foreach ($kind in $entry.Kinds) {
        $rows.Add([ordered]@{ id = 'when'; code = $kind })
    }
    $state = 'off'
    if ($entry.On) {
        $state = 'on'
    }
    $rows.Add([ordered]@{ id = 'state'; code = $state })
    if ($entry.Last.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'last'; value = $entry.Last })
    }
    foreach ($flag in $entry.Flags) {
        $rows.Add([ordered]@{ id = 'flag'; code = $flag })
    }
    $sections.Add([ordered]@{ id = 'task'; name = $entry.Label; rows = $rows.ToArray() })
}

$result = 'none'
if ($flaggedCount -gt 0) {
    $result = 'flagged'
}
elseif ($entries.Count -gt 0) {
    $result = 'found'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{ tasks = $entries.Count; flagged = $flaggedCount }
    sections = $sections.ToArray()
}
