# Tool: system.installed-programs (info)
# The desktop programs installed on this PC, as "Installed apps" in Settings
# and "Programs and Features" list them: the Uninstall keys of HKLM, of its
# 32-bit view, and of the logged-on user's hive (-UserHive). Three tables:
#   recent   installed in the last $recentDays days, newest first (at most
#            $maxShown): a program one does not remember installing was often
#            bundled with something else
#   broken   the uninstaller is gone (UninstallString names an absolute path
#            that does not exist), so uninstalling it from Settings fails (at
#            most $maxShown). MSI packages are not checked (msiexec is always
#            there; their cached package cannot be checked from here), nor
#            commands with %VARIABLES% (they would expand with the values of
#            the administrator running this, not of the logged-on user)
#   biggest  the largest by EstimatedSize (at most $maxBiggest)
# Listed as Settings lists them: entries with a DisplayName, not
# SystemComponent = 1, not updates (ParentKeyName, or ReleaseType Update /
# Hotfix / Security Update); a program registered twice (same name and
# version) is listed once.
# Read-only. Privacy: no install path, uninstall command or key name is
# output, only each program's name, publisher, version, install date and size.
# Store apps are not in these keys (Settings lists them).
# Result: listed. Facts: count, recent_count, broken_count, days.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$recentDays = 30
$maxShown = 15
$maxBiggest = 10
$invariant = [Globalization.CultureInfo]::InvariantCulture

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return (([string]$Value) -replace '[\x00-\x1f]', ' ').Trim()
}

function Get-Prop {
    param($Object, [string]$Name)
    if ($null -eq $Object) {
        return ''
    }
    $prop = $Object.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return ''
    }
    return Get-Text $prop.Value
}

# InstallDate is usually yyyyMMdd; some installers write other forms.
function Get-InstallDate {
    param([string]$Text)
    $formats = [string[]]@('yyyyMMdd', 'yyyy-MM-dd', 'yyyy/M/d', 'yyyy/MM/dd', 'M/d/yyyy', 'MM/dd/yyyy')
    $date = [datetime]::MinValue
    if ([datetime]::TryParseExact($Text, $formats, $invariant, [Globalization.DateTimeStyles]::None, [ref]$date)) {
        if (($date.Year -ge 1995) -and ($date -le (Get-Date).AddDays(1))) {
            return $date
        }
    }
    return $null
}

# The program an uninstall command runs: the quoted path, or up to ".exe".
function Get-CommandPath {
    param([string]$Command)
    $text = $Command.Trim()
    if ($text.StartsWith('"')) {
        $end = $text.IndexOf('"', 1)
        if ($end -gt 1) {
            return $text.Substring(1, $end - 1)
        }
        return ''
    }
    $match = [regex]::Match($text, '^(?i)(.+?\.exe)(\s|$)')
    if ($match.Success) {
        return $match.Groups[1].Value
    }
    return $text
}

function Test-UninstallerMissing {
    param($Entry)
    if ((Get-Prop $Entry 'WindowsInstaller') -eq '1') {
        return $false
    }
    $command = Get-Prop $Entry 'UninstallString'
    if (($command.Length -eq 0) -or ($command -match '(?i)msiexec') -or $command.Contains('%')) {
        return $false
    }
    $path = Get-CommandPath $command
    if ($path -notmatch '^[A-Za-z]:\\') {
        return $false
    }
    return (-not (Test-Path -LiteralPath $path -PathType Leaf))
}

function Format-Size {
    param([int64]$Kb)
    if ($Kb -ge 1048576) {
        return [string]::Format($invariant, '{0:0.0} GB', $Kb / 1048576.0)
    }
    if ($Kb -ge 1024) {
        return [string]::Format($invariant, '{0:0} MB', $Kb / 1024.0)
    }
    return [string]::Format($invariant, '{0} KB', $Kb)
}

$roots = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
    ($UserHive.TrimEnd('\') + '\Software\Microsoft\Windows\CurrentVersion\Uninstall')
)

$seen = @{}
$programs = New-Object System.Collections.Generic.List[object]
foreach ($root in $roots) {
    if (-not (Test-Path -LiteralPath $root)) {
        continue
    }
    foreach ($key in @(Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)) {
        $entry = $null
        try {
            $entry = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction Stop
        }
        catch {
            continue
        }
        # A key without values gives no object at all
        if ($null -eq $entry) {
            continue
        }
        $name = Get-Prop $entry 'DisplayName'
        if (($name.Length -eq 0) -or ((Get-Prop $entry 'SystemComponent') -eq '1') -or ((Get-Prop $entry 'ParentKeyName').Length -gt 0)) {
            continue
        }
        if (@('Update', 'Hotfix', 'Security Update') -contains (Get-Prop $entry 'ReleaseType')) {
            continue
        }
        $version = Get-Prop $entry 'DisplayVersion'
        $id = ($name + '|' + $version).ToLowerInvariant()
        if ($seen.ContainsKey($id)) {
            continue
        }
        $seen[$id] = $true
        $sizeKb = [int64]0
        $sizeText = Get-Prop $entry 'EstimatedSize'
        if ($sizeText -match '^\d+$') {
            $sizeKb = [int64]$sizeText
        }
        $programs.Add([pscustomobject]@{
                Name      = $name
                Publisher = Get-Prop $entry 'Publisher'
                Version   = $version
                Date      = Get-InstallDate (Get-Prop $entry 'InstallDate')
                SizeKb    = $sizeKb
                Broken    = Test-UninstallerMissing $entry
            })
    }
}

function New-Rows {
    param($Program, [string[]]$Fields)
    $rows = New-Object System.Collections.Generic.List[object]
    foreach ($field in $Fields) {
        switch ($field) {
            'publisher' {
                if ($Program.Publisher.Length -gt 0) {
                    $rows.Add([ordered]@{ id = 'publisher'; value = $Program.Publisher })
                }
            }
            'version' {
                if ($Program.Version.Length -gt 0) {
                    $rows.Add([ordered]@{ id = 'version'; value = $Program.Version })
                }
            }
            'installed' {
                if ($null -ne $Program.Date) {
                    $rows.Add([ordered]@{ id = 'installed'; value = $Program.Date.ToString('yyyy-MM-dd', $invariant) })
                }
            }
            'size' {
                if ($Program.SizeKb -gt 0) {
                    $rows.Add([ordered]@{ id = 'size'; value = (Format-Size $Program.SizeKb) })
                }
            }
            'advice' {
                $rows.Add([ordered]@{ id = 'advice'; code = 'reinstall' })
            }
        }
    }
    return $rows.ToArray()
}

$since = (Get-Date).Date.AddDays(-$recentDays)
$recent = @($programs | Where-Object { ($null -ne $_.Date) -and ($_.Date -ge $since) } | Sort-Object -Property Date -Descending)
$broken = @($programs | Where-Object { $_.Broken } | Sort-Object -Property Name)
$biggest = @($programs | Where-Object { $_.SizeKb -gt 0 } | Sort-Object -Property SizeKb -Descending | Select-Object -First $maxBiggest)

$sections = New-Object System.Collections.Generic.List[object]
foreach ($p in @($recent | Select-Object -First $maxShown)) {
    $sections.Add([ordered]@{ id = 'recent'; name = $p.Name; rows = (New-Rows $p @('publisher', 'version', 'installed', 'size')) })
}
foreach ($p in @($broken | Select-Object -First $maxShown)) {
    $sections.Add([ordered]@{ id = 'broken'; name = $p.Name; rows = (New-Rows $p @('publisher', 'installed', 'advice')) })
}
foreach ($p in $biggest) {
    $sections.Add([ordered]@{ id = 'biggest'; name = $p.Name; rows = (New-Rows $p @('size', 'publisher', 'installed')) })
}

[pscustomobject]@{
    result   = 'listed'
    facts    = [ordered]@{
        count        = $programs.Count
        recent_count = $recent.Count
        broken_count = $broken.Count
        days         = $recentDays
    }
    sections = $sections.ToArray()
}
