# Check: system.pagefile
# Is there a page file ("virtual memory")? HKLM\SYSTEM\CurrentControlSet\
# Control\Session Manager\Memory Management, PagingFiles (REG_MULTI_SZ), one
# line per page file (Microsoft Japan Windows support blog, "How to change the
# paging file settings"):
#   ?:\pagefile.sys             managed by Windows on all drives (the default)
#   c:\pagefile.sys 0 0         managed by Windows on that drive
#   c:\pagefile.sys 2048 4096   sizes chosen by hand (MB)
# No line at all is "No paging file" on every drive, which many "optimizing"
# guides and tools suggest. The commit limit is then the physical memory
# alone: when programs reach it they freeze or crash ("out of memory"), and a
# blue screen leaves no crash dump (Microsoft, "Introduction to the page
# file").
# The value is what the next start of Windows uses (changes apply after a
# restart), and that is what is reported.
# Result codes: auto (managed by Windows on all drives) / custom (set by hand,
# one of them on the system drive) / other-drive (only on other drives: fine
# for memory, but no crash dump) / off (none) / missing (the value is not
# there; not judged).
# Facts: files (the lines, comma separated; empty when there are none).
# Read-only.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management'

$lines = $null
try {
    $item = Get-ItemProperty -LiteralPath $key -Name 'PagingFiles' -ErrorAction Stop
    $lines = @($item.PagingFiles)
}
catch {
    $lines = $null
}
if ($null -eq $lines) {
    [pscustomobject]@{ result = 'missing'; facts = [ordered]@{ files = '' } }
    return
}

$files = New-Object System.Collections.Generic.List[string]
$auto = $false
$onSystem = $false
$systemDrive = ([string]$env:SystemDrive).TrimEnd('\').ToUpperInvariant()
if ($systemDrive.Length -eq 0) {
    $systemDrive = 'C:'
}
foreach ($line in $lines) {
    $text = ([string]$line).Trim()
    if ($text.Length -eq 0) {
        continue
    }
    $files.Add($text)
    if ($text -match '^\?:') {
        $auto = $true
    }
    elseif ($text -match '^([A-Za-z]:)') {
        if ($Matches[1].ToUpperInvariant() -eq $systemDrive) {
            $onSystem = $true
        }
    }
}

$result = 'off'
if ($auto) {
    $result = 'auto'
}
elseif ($onSystem) {
    $result = 'custom'
}
elseif ($files.Count -gt 0) {
    $result = 'other-drive'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ files = ($files.ToArray() -join ', ') }
}
