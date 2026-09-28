# Check: disk.old-drivers
# Old versions of third-party drivers kept in the driver store
# (C:\Windows\System32\DriverStore\FileRepository): every graphics or audio
# driver update leaves the previous package behind, often hundreds of MB each.
# Which ones are old is decided by the shared block below (the rules of
# "Select Old Drivers" in Driver Store Explorer, a little more careful).
# Read-only; needs administrator rights (Get-WindowsDriver).
# Result codes:
#   none   no old driver package
#   small  some, but less than 100 MB in all: not worth a clean-up
#   found  100 MB or more
# Facts: count, size_mb, size_gb, classes (driver classes, e.g. Display,
# MEDIA), providers (up to 3 driver providers, e.g. NVIDIA); total and
# multi (third-party packages, and groups with more than one version) for
# the logs. No paths are output.

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

$found = Get-OldDriver
$old = @($found.Old)
$bytes = [long]0
foreach ($o in $old) {
    $bytes += [long]$o.Bytes
}
$classes = @($old | ForEach-Object { $_.ClassName } | Where-Object { $_ } | Sort-Object -Unique)
$providers = @($old | Group-Object -Property ProviderName | Sort-Object -Property Count -Descending |
        Select-Object -First 3 | ForEach-Object { $_.Name } | Where-Object { $_ })

$result = 'none'
if ($old.Count -gt 0) {
    $result = 'small'
    if ($bytes -ge 100MB) {
        $result = 'found'
    }
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        count     = $old.Count
        size_mb   = [math]::Round($bytes / 1MB)
        size_gb   = [math]::Round($bytes / 1GB, 1)
        classes   = ($classes -join ', ')
        providers = ($providers -join ', ')
        total     = $found.All
        multi     = $found.Multi
    }
}
