# Feature: hardware.class-filters-repair -- run (and prepare)
# Takes the filters that are gone out of the class lists and puts the
# keyboard and mouse class drivers back (see the shared block); filters that
# work stay, in their order. A list left empty is removed (Windows treats a
# missing list as "no filters"). Windows builds the driver stacks again when
# it restarts.
# -Prepare: returns before = { lists: [ { code, value, current (the names as
#   they were, or null when the value was not there), fixed (the names it gets,
#   empty when it is removed) } ] }.
# Run: -Before is that JSON. Returns skipped (nothing changed) when there is
#   nothing to do or a list changed since. Otherwise writes the lists
#   (REG_MULTI_SZ) and reads them back; when one fails, the lists already
#   written are put back and the script throws.

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block class-filters: identical in checks/hardware/class-filters.ps1 and features/hardware/class-filters-*.ps1 (medkit-data check compares them) ----
# The classes: code, class GUID, the class driver that must stay in
# UpperFilters ('' when there is none).
$filterClasses = @(
    @('keyboard', '{4d36e96b-e325-11ce-bfc1-08002be10318}', 'kbdclass'),
    @('mouse', '{4d36e96f-e325-11ce-bfc1-08002be10318}', 'mouclass'),
    @('cdrom', '{4d36e965-e325-11ce-bfc1-08002be10318}', ''),
    @('usb', '{36fc9e60-c465-11cf-8056-444553540000}', ''),
    @('image', '{6bdd1fc6-810f-11d0-bec7-08002be2092f}', ''),
    @('camera', '{ca3e7ab9-b4c3-4ae6-8251-579ef933890f}', '')
)
$filterValueNames = @('UpperFilters', 'LowerFilters')
$classRoot = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\'
$serviceRoot = 'HKLM:\SYSTEM\CurrentControlSet\Services\'

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$filterSystemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $filterSystemDir = Join-Path $env:windir 'Sysnative'
}

# A filter list as it is: $null when the value is not there, else the names
# (a single string counts as one name).
function Get-FilterList {
    param([string]$Key, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Key -ErrorAction Stop
    }
    catch {
        return $null
    }
    if ($null -eq $item) {
        return $null
    }
    $property = $item.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return , @(@($property.Value) | ForEach-Object { [string]$_ } | Where-Object { $_.Trim().Length -gt 0 })
}

# The file of a kernel driver's ImagePath, or '' when it cannot be told.
function Get-DriverFile {
    param([string]$Name, [string]$ImagePath)
    $path = $ImagePath.Trim().Trim('"')
    if ($path.Length -eq 0) {
        return $filterSystemDir + '\drivers\' + $Name + '.sys'
    }
    if ($path -match '^\\SystemRoot\\(.*)$') {
        $path = $env:windir + '\' + $Matches[1]
    }
    elseif ($path -match '^\\\?\?\\([A-Za-z]:\\.*)$') {
        $path = $Matches[1]
    }
    elseif ($path -match '^(?i)system32\\(.*)$') {
        $path = $env:windir + '\System32\' + $Matches[1]
    }
    elseif ($path -notmatch '^[A-Za-z]:\\') {
        return ''
    }
    if ($path -match '^(?i)(.*)\\System32\\(.*)$') {
        if ([string]::Equals($Matches[1], $env:windir, [StringComparison]::OrdinalIgnoreCase)) {
            $path = $filterSystemDir + '\' + $Matches[2]
        }
    }
    return $path
}

# A filter is left behind when its service is gone (no key, or a key without
# any values), or it is a kernel driver (Type 1) whose file is not there.
function Test-FilterGone {
    param([string]$Name)
    $key = $serviceRoot + $Name
    if (-not (Test-Path -LiteralPath $key)) {
        return $true
    }
    $service = Get-ItemProperty -LiteralPath $key
    if ($null -eq $service) {
        # A service key without any values: what an uninstaller left behind.
        return $true
    }
    $type = $service.PSObject.Properties['Type']
    if (($null -eq $type) -or ([int64]$type.Value -ne 1)) {
        return $false
    }
    $image = $service.PSObject.Properties['ImagePath']
    $file = Get-DriverFile $Name ([string]$(if ($null -ne $image) { $image.Value } else { '' }))
    if ($file.Length -eq 0) {
        return $false
    }
    return (-not (Test-Path -LiteralPath $file -PathType Leaf))
}

# Every class list with a problem: Code, Value (UpperFilters / LowerFilters),
# Current (the names, or $null when the value is not there), Gone (the names
# left behind), Missing (the class driver that is not there, or ''), Fixed
# (the list without what is left behind, with the class driver).
function Get-FilterProblems {
    $problems = New-Object System.Collections.Generic.List[object]
    foreach ($class in $filterClasses) {
        $key = $classRoot + $class[1]
        if (-not (Test-Path -LiteralPath $key)) {
            continue
        }
        foreach ($valueName in $filterValueNames) {
            $current = Get-FilterList $key $valueName
            $gone = New-Object System.Collections.Generic.List[string]
            $kept = New-Object System.Collections.Generic.List[string]
            foreach ($name in @($current)) {
                if ($null -eq $name) {
                    continue
                }
                if ((-not [string]::Equals($name, $class[2], [StringComparison]::OrdinalIgnoreCase)) -and (Test-FilterGone $name)) {
                    $gone.Add($name)
                }
                else {
                    $kept.Add($name)
                }
            }
            $missing = ''
            if (($valueName -eq 'UpperFilters') -and ($class[2].Length -gt 0)) {
                $has = @($kept | Where-Object { [string]::Equals($_, $class[2], [StringComparison]::OrdinalIgnoreCase) }).Count -gt 0
                if (-not $has) {
                    $missing = $class[2]
                    $kept.Add($class[2])
                }
            }
            if (($gone.Count -eq 0) -and ($missing.Length -eq 0)) {
                continue
            }
            $problems.Add([pscustomobject]@{
                    Code    = $class[0]
                    Key     = $key
                    Value   = $valueName
                    Current = $current
                    Gone    = $gone.ToArray()
                    Missing = $missing
                    Fixed   = $kept.ToArray()
                })
        }
    }
    return , $problems.ToArray()
}
# ---- end of shared block class-filters ----

# The names in a list, compared without regard to case and order-sensitive.
function Test-SameList {
    param($Left, $Right)
    if (($null -eq $Left) -or ($null -eq $Right)) {
        return (($null -eq $Left) -and ($null -eq $Right))
    }
    $a = @($Left)
    $b = @($Right)
    if ($a.Count -ne $b.Count) {
        return $false
    }
    for ($i = 0; $i -lt $a.Count; $i++) {
        if (-not [string]::Equals([string]$a[$i], [string]$b[$i], [StringComparison]::OrdinalIgnoreCase)) {
            return $false
        }
    }
    return $true
}

# Writes a list: the names as REG_MULTI_SZ, or removes the value when there
# are none ($null or empty).
function Set-FilterList {
    param([string]$Key, [string]$Name, $Names)
    $list = @(@($Names) | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_ })
    if ($list.Count -eq 0) {
        Remove-ItemProperty -LiteralPath $Key -Name $Name -ErrorAction SilentlyContinue
        return
    }
    Set-ItemProperty -LiteralPath $Key -Name $Name -Value ([string[]]$list) -Type MultiString -ErrorAction Stop
}

# The registry key of a class code.
function Get-ClassKey {
    param([string]$Code)
    $class = @($filterClasses | Where-Object { $_[0] -eq $Code })
    if ($class.Count -eq 0) {
        throw ('unknown class ' + $Code)
    }
    return $classRoot + $class[0][1]
}

$problems = Get-FilterProblems
if ($Prepare) {
    $lists = @(foreach ($problem in $problems) {
            [ordered]@{
                code    = $problem.Code
                value   = $problem.Value
                current = $problem.Current
                fixed   = @($problem.Fixed)
            }
        })
    return [pscustomobject]@{ before = [ordered]@{ lists = $lists } }
}

$recorded = @(@((ConvertFrom-Json -InputObject $Before).lists) | Where-Object { $null -ne $_ })
if ($recorded.Count -eq 0) {
    return [pscustomobject]@{ skipped = $true }
}
foreach ($record in $recorded) {
    $key = Get-ClassKey ([string]$record.code)
    $now = Get-FilterList $key ([string]$record.value)
    if (-not (Test-SameList $now $record.current)) {
        return [pscustomobject]@{ skipped = $true }
    }
}

$written = New-Object System.Collections.Generic.List[object]
try {
    foreach ($record in $recorded) {
        $key = Get-ClassKey ([string]$record.code)
        $written.Add($record)
        Set-FilterList $key ([string]$record.value) $record.fixed
    }
    $left = Get-FilterProblems
    if ($left.Count -gt 0) {
        throw 'a class list still names a filter that is gone'
    }
}
catch {
    $failure = $_
    foreach ($record in $written) {
        try {
            Set-FilterList (Get-ClassKey ([string]$record.code)) ([string]$record.value) $record.current
        }
        catch {
            Write-Verbose ('could not put the list back: ' + $_.Exception.Message)
        }
    }
    throw $failure
}

[pscustomobject]@{ after = [ordered]@{ fixed = $recorded.Count } }
