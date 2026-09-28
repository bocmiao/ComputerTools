# Check: system.command-path
# Can the command prompt find the commands that come with Windows (ping,
# ipconfig, ...)? When the machine's Path or PATHEXT environment variable is
# broken, every one of them says "is not recognized as an internal or external
# command". Read-only; the machine part is repaired by
# system.command-path-repair (see the shared block).
# Also read: a PATHEXT of the logged-in user's own (-UserHive, Environment),
# which replaces the machine's for that user; it is only reported.
# Result codes (the first that applies): path-type / path-missing / pathext
# (fix: system.command-path-repair) / user-pathext (advice) / ok.
# Facts: missing (codes of the Windows folders not in Path: system32, windows,
# wbem, powershell), missing_ext (the extensions missing, like ".EXE"),
# path_kind (expand / string / none).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

# ---- shared block command-path: identical in checks/system/command-path.ps1 and features/system/command-path-*.ps1 (medkit-data check compares them) ----
# The machine's environment variables are in
# HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment:
# - Path must be REG_EXPAND_SZ, or %SystemRoot% in it is not replaced
#   (Microsoft, "Path Entry"), and holds the Windows folders
#   ($defaultEntries: the Path of a new Windows 10 / 11, OpenSSH aside).
# - PATHEXT lists the extensions the command prompt tries for a bare command
#   name; its default is $defaultPathExt (Microsoft, "start").
# The repair puts the missing Windows folders (those that exist) in front of
# the others, which stay as they were and in their order, and writes Path as
# REG_EXPAND_SZ; it puts the missing extensions of $neededExt back into
# PATHEXT (the default list first, then any others it had).
$environmentKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
# Code, entry as Windows writes it.
$defaultEntries = @(
    @('system32', '%SystemRoot%\system32'),
    @('windows', '%SystemRoot%'),
    @('wbem', '%SystemRoot%\System32\Wbem'),
    @('powershell', '%SYSTEMROOT%\System32\WindowsPowerShell\v1.0\')
)
$defaultPathExt = @('.COM', '.EXE', '.BAT', '.CMD', '.VBS', '.VBE', '.JS', '.JSE', '.WSF', '.WSH', '.MSC')
$neededExt = @('.COM', '.EXE', '.BAT', '.CMD')

# A value as stored (not expanded) and its kind: @{ present; value; kind }
# (kind: ExpandString, String, ...).
function Get-EnvValue {
    param([string]$Key, [string]$Name)
    $state = [ordered]@{ present = $false; value = ''; kind = '' }
    try {
        $item = Get-Item -LiteralPath $Key -ErrorAction Stop
    }
    catch {
        return $state
    }
    if (@($item.GetValueNames()) -notcontains $Name) {
        return $state
    }
    $state.present = $true
    $state.value = [string]$item.GetValue($Name, '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $state.kind = [string]$item.GetValueKind($Name)
    return $state
}

# The non-empty entries of a list separated by ";".
function Split-List {
    param([string]$Text)
    return , @(@($Text -split ';') | ForEach-Object { $_.Trim() } | Where-Object { $_.Length -gt 0 })
}

# A Path entry as a folder to compare: variables expanded, no trailing "\",
# lower case.
function Get-EntryKey {
    param([string]$Entry)
    return [Environment]::ExpandEnvironmentVariables($Entry.Trim().Trim('"')).TrimEnd('\').ToLowerInvariant()
}

# The codes and entries of the Windows folders missing from a Path text; only
# folders that exist count (the PowerShell folder can be removed).
function Get-MissingEntries {
    param([string]$PathText)
    $have = @{}
    foreach ($entry in (Split-List $PathText)) {
        $have[(Get-EntryKey $entry)] = $true
    }
    $missing = New-Object System.Collections.Generic.List[object]
    foreach ($pair in $defaultEntries) {
        $folder = [Environment]::ExpandEnvironmentVariables($pair[1]).TrimEnd('\')
        if ($have.ContainsKey($folder.ToLowerInvariant())) {
            continue
        }
        if (($pair[0] -ne 'system32') -and (-not (Test-Path -LiteralPath $folder -PathType Container))) {
            continue
        }
        $missing.Add($pair)
    }
    return , $missing.ToArray()
}

# The needed extensions a PATHEXT text lacks.
function Get-MissingExt {
    param([string]$Text)
    $have = @{}
    foreach ($ext in (Split-List $Text)) {
        $have[$ext.ToUpperInvariant()] = $true
    }
    return , @($neededExt | Where-Object { -not $have.ContainsKey($_) })
}

# What is wrong with the machine's Path and PATHEXT (states from
# Get-EnvValue): @{ PathType; Missing; MissingExt }. PathType is true when
# Path is not REG_EXPAND_SZ but uses %...%.
function Get-CommandPathProblems {
    param($PathState, $PathExtState)
    $pathType = $PathState.present -and ($PathState.kind -ne 'ExpandString') -and ($PathState.value.Contains('%'))
    return [pscustomobject]@{
        PathType   = $pathType
        Missing    = Get-MissingEntries $PathState.value
        MissingExt = Get-MissingExt $PathExtState.value
    }
}

# The Path text the repair writes: the missing Windows folders first, then
# the entries it had, as they were.
function Get-FixedPath {
    param([string]$PathText)
    $entries = New-Object System.Collections.Generic.List[string]
    foreach ($pair in (Get-MissingEntries $PathText)) {
        $entries.Add($pair[1])
    }
    foreach ($entry in (Split-List $PathText)) {
        $entries.Add($entry)
    }
    return ($entries -join ';')
}

# The PATHEXT text the repair writes: the default list, then the other
# extensions it had, in their order.
function Get-FixedPathExt {
    param([string]$Text)
    $entries = New-Object System.Collections.Generic.List[string]
    foreach ($ext in $defaultPathExt) {
        $entries.Add($ext)
    }
    foreach ($ext in (Split-List $Text)) {
        if ($defaultPathExt -notcontains $ext.ToUpperInvariant()) {
            $entries.Add($ext)
        }
    }
    return ($entries -join ';')
}
# ---- end of shared block command-path ----

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

$pathState = Get-EnvValue $environmentKey 'Path'
$pathExtState = Get-EnvValue $environmentKey 'PATHEXT'
$problems = Get-CommandPathProblems $pathState $pathExtState
$userExt = Get-EnvValue ($UserHive.TrimEnd('\') + '\Environment') 'PATHEXT'
$userMissing = @()
if ($userExt.present) {
    $userMissing = Get-MissingExt $userExt.value
}

$kind = 'none'
if ($pathState.present) {
    $kind = 'string'
    if ($pathState.kind -eq 'ExpandString') {
        $kind = 'expand'
    }
}
$missingExt = @($problems.MissingExt)
$result = 'ok'
if ($problems.PathType) {
    $result = 'path-type'
}
elseif (@($problems.Missing).Count -gt 0) {
    $result = 'path-missing'
}
elseif ($missingExt.Count -gt 0) {
    $result = 'pathext'
}
elseif (@($userMissing).Count -gt 0) {
    $result = 'user-pathext'
    $missingExt = @($userMissing)
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        missing     = @(@($problems.Missing) | ForEach-Object { $_[0] })
        missing_ext = @($missingExt)
        path_kind   = $kind
    }
}
