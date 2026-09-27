# Remove regular files older than seven days from Windows Temp and the signed-in
# user's standard LocalAppData\Temp. Do not descend into reparse points.
[CmdletBinding()]
param([string]$UserHive = 'HKCU:')

$ErrorActionPreference = 'Stop'
$cutoff = (Get-Date).AddDays(-7)
$systemDrive = [IO.Path]::GetPathRoot($env:WINDIR)
$targets = New-Object System.Collections.Generic.List[string]
$targets.Add((Join-Path $env:WINDIR 'Temp'))

$profilePath = $null
if ($UserHive -match 'HKEY_USERS\\(S-\d+(?:-\d+)+)$') {
    $sid = $Matches[1]
    $profileKey = 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $sid
    try {
        $profilePath = (Get-ItemProperty -LiteralPath $profileKey -Name ProfileImagePath).ProfileImagePath
        $profilePath = [Environment]::ExpandEnvironmentVariables($profilePath)
    }
    catch {
        $profilePath = $null
    }
}
if (-not [string]::IsNullOrWhiteSpace($profilePath)) {
    $userTemp = Join-Path $profilePath 'AppData\Local\Temp'
    $profileItem = Get-Item -LiteralPath $profilePath -Force -ErrorAction SilentlyContinue
    if (($null -ne $profileItem) -and
        (($profileItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) -and
        ([IO.Path]::GetPathRoot($userTemp) -ieq $systemDrive)) {
        $targets.Add($userTemp)
    }
}

$deleted = 0
$skipped = 0
$freed = [long]0
foreach ($target in $targets) {
    if (-not (Test-Path -LiteralPath $target -PathType Container)) { continue }
    $root = Get-Item -LiteralPath $target -Force
    if (($root.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
    $pending = New-Object 'System.Collections.Generic.Stack[string]'
    $pending.Push($root.FullName)
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        try {
            $children = @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop)
        }
        catch {
            $skipped++
            continue
        }
        foreach ($child in $children) {
            if (($child.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
            if ($child.PSIsContainer) {
                $pending.Push($child.FullName)
                continue
            }
            if ($child.LastWriteTime -ge $cutoff) { continue }
            try {
                $size = [long]$child.Length
                Remove-Item -LiteralPath $child.FullName -Force -ErrorAction Stop
                $freed += $size
                $deleted++
            }
            catch {
                $skipped++
            }
        }
    }
}

[pscustomobject]@{
    result = $(if ($deleted -gt 0) { 'done' } elseif ($skipped -gt 0) { 'partial' } else { 'empty' })
    facts = [ordered]@{
        deleted = $deleted
        skipped = $skipped
        freed_mb = [math]::Round($freed / 1MB, 1)
    }
}
