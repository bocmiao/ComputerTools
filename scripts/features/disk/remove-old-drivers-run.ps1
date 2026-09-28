# Feature: disk.remove-old-drivers -- run (cannot be undone: undo: none)
# Deletes the old driver packages the shared block finds, one by one, with
# pnputil /delete-driver <oemNN.inf> (Microsoft, "PnPUtil Command Syntax";
# Windows 10 1607 and later). Without /force pnputil refuses a package that a
# device uses, so nothing in use can go even if the list were wrong; such a
# package is skipped and counted. Only names like oemNN.inf are passed on.
# The check disk.old-drivers reads the state before and after (verify).
# Returns after = { deleted, failed, freed_mb }; skipped when there is nothing
# to delete; throws when none of the old packages could be deleted.
# pnputil prints localized lines; only its exit code is judged (0, or 3010:
# done, a restart finishes it), and its text goes into the error message.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block old-drivers: identical in checks/disk/old-drivers.ps1 and features/disk/remove-old-drivers-run.ps1 (medkit-data check compares them) ----
# Old driver packages, chosen like "Select Old Drivers" in Driver Store
# Explorer (lostindark/DriverStoreExplorer; only its rules are followed, no
# code is taken):
#   - only third-party packages (Get-WindowsDriver -Online without -All: the
#     oemNN.inf packages; Windows' own drivers are never touched);
#   - boot-critical packages, ntprint.inf, extension INFs (class Extension:
#     they belong to a device by an extension id this list cannot see),
#     printers and print queues, and packages whose version cannot be read
#     are left alone;
#   - packages are grouped by class, provider and original INF file name; in
#     a group with more than one version the newest version (with any
#     identical copies) stays, and so does any package dated after it (its
#     version number may not tell the truth);
#   - of the rest, only packages that no device uses are old: no InfPath of a
#     device instance under Control\Class (devices plugged in or not) names
#     it. pnputil /delete-driver without /force refuses a package in use as
#     well.
$skippedClasses = @('Extension', 'Printer', 'PrintQueue')

# Every INF that an installed device uses, present or not: the InfPath of
# each device instance (Control\Class\{class}\NNNN).
function Get-UsedInfs {
    $used = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $classRoot = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class'
    foreach ($class in @(Get-ChildItem -LiteralPath $classRoot -ErrorAction SilentlyContinue)) {
        foreach ($instance in @(Get-ChildItem -LiteralPath $class.PSPath -ErrorAction SilentlyContinue)) {
            if ($instance.PSChildName -notmatch '^\d{4}$') {
                continue
            }
            $inf = $instance.GetValue('InfPath')
            if ($null -ne $inf -and ([string]$inf).Length -gt 0) {
                [void]$used.Add([string]$inf)
            }
        }
    }
    return , $used
}

# The size in bytes of a folder and everything in it (0 when unreadable).
function Get-FolderByte {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        return [long]0
    }
    $sum = (Get-ChildItem -LiteralPath $Path -Recurse -File -Force -ErrorAction SilentlyContinue |
            Measure-Object -Property Length -Sum).Sum
    if ($null -eq $sum) {
        return [long]0
    }
    return [long]$sum
}

# The third-party packages ($All) and the old ones ($Old): objects with
# Driver (oemNN.inf), ClassName, ProviderName and Bytes; plus the number of
# groups that have more than one version ($Multi).
function Get-OldDriver {
    $drivers = @(Get-WindowsDriver -Online -ErrorAction Stop)
    $used = Get-UsedInfs
    $groups = @{}
    foreach ($d in $drivers) {
        if ($d.BootCritical) {
            continue
        }
        $class = [string]$d.ClassName
        if ($skippedClasses -contains $class) {
            continue
        }
        $original = [string]$d.OriginalFileName
        if ($original.Length -eq 0) {
            continue
        }
        $leaf = Split-Path -Path $original -Leaf
        if ($leaf -ieq 'ntprint.inf') {
            continue
        }
        $version = $null
        if (-not [version]::TryParse([string]$d.Version, [ref]$version)) {
            continue
        }
        $key = ($class + '|' + [string]$d.ProviderName + '|' + $leaf).ToLowerInvariant()
        if (-not $groups.ContainsKey($key)) {
            $groups[$key] = New-Object System.Collections.ArrayList
        }
        [void]$groups[$key].Add([pscustomobject]@{
                Driver       = [string]$d.Driver
                ClassName    = $class
                ProviderName = [string]$d.ProviderName
                Version      = $version
                Date         = $d.Date
                Folder       = (Split-Path -Path $original -Parent)
            })
    }
    $old = New-Object System.Collections.ArrayList
    $multi = 0
    foreach ($list in $groups.Values) {
        if ($list.Count -lt 2) {
            continue
        }
        $multi++
        $sorted = @($list | Sort-Object -Property @{ Expression = 'Version'; Descending = $true }, @{ Expression = 'Date'; Descending = $true })
        $newest = $sorted[0]
        foreach ($p in $sorted) {
            if (($p.Version -eq $newest.Version) -and ($p.Date -eq $newest.Date)) {
                continue
            }
            if ($p.Date -gt $newest.Date) {
                continue
            }
            if ($used.Contains($p.Driver)) {
                continue
            }
            [void]$old.Add([pscustomobject]@{
                    Driver       = $p.Driver
                    ClassName    = $p.ClassName
                    ProviderName = $p.ProviderName
                    Bytes        = (Get-FolderByte $p.Folder)
                })
        }
    }
    return [pscustomobject]@{ All = $drivers.Count; Multi = $multi; Old = $old.ToArray() }
}
# ---- end of shared block old-drivers ----

$old = @((Get-OldDriver).Old)
if ($old.Count -eq 0) {
    return [pscustomobject]@{ skipped = $true }
}

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$pnputil = Join-Path $systemDir 'pnputil.exe'
if (-not (Test-Path -LiteralPath $pnputil -PathType Leaf)) {
    throw 'pnputil.exe was not found'
}

$deleted = 0
$failed = 0
$freed = [long]0
$lastError = ''
foreach ($o in $old) {
    if ($o.Driver -notmatch '^oem\d+\.inf$') {
        $failed++
        continue
    }
    # Windows PowerShell 5.1 would turn a stderr line into a terminating error
    # under 'Stop', so relax it and judge by the exit code.
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = @(& $pnputil '/delete-driver' $o.Driver 2>&1 | ForEach-Object { [string]$_ })
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if (($exitCode -eq 0) -or ($exitCode -eq 3010)) {
        $deleted++
        $freed += [long]$o.Bytes
    }
    else {
        $failed++
        $lastError = ('{0}: exit code {1}: {2}' -f $o.Driver, $exitCode, (($output | Where-Object { $_.Trim().Length -gt 0 }) -join ' ').Trim())
    }
}
if ($deleted -eq 0) {
    throw ('pnputil deleted none of the {0} old driver packages. {1}' -f $old.Count, $lastError)
}

[pscustomobject]@{
    after = [pscustomobject]@{
        deleted  = $deleted
        failed   = $failed
        freed_mb = [math]::Round($freed / 1MB)
    }
}
