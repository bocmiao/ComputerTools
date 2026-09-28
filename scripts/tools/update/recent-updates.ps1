# Tool: update.recent-updates (info)
# The updates installed in the last $days days, day by day, newest first, to
# tell which update came right before a problem started. Read-only.
# From the Windows Update history (Windows Update Agent:
# Microsoft.Update.Session, QueryHistory): installations (Operation 1) that
# succeeded (ResultCode 2, or 3 with errors), with their title as Windows
# Update shows it ("2026-09 ... (KB5065426)", in the language of Windows).
# The virus definition updates of Microsoft Defender (KB2267602, several a
# day) are left out. When the history cannot be read (Windows Update is
# disabled: 0x80070422), the installed hotfixes are listed instead
# (Get-HotFix, from Win32_QuickFixEngineering: the KB number and the day).
# At most $maxUpdates updates. Titles are cut at 120 characters.
# Result codes: found / none. Facts: days, count, source (history /
# hotfixes).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 30
$maxUpdates = 20
$maxEntries = 300
$pageSize = 50
$skipped = @('KB2267602', 'KB915597')

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

$since = (Get-Date).Date.AddDays(1 - $days)
$updates = New-Object System.Collections.Generic.List[object]
$source = 'history'
try {
    $session = New-Object -ComObject Microsoft.Update.Session
    $searcher = $session.CreateUpdateSearcher()
    $total = [math]::Min([int]$searcher.GetTotalHistoryCount(), $maxEntries)
    for ($start = 0; $start -lt $total; $start += $pageSize) {
        $page = @($searcher.QueryHistory($start, [math]::Min($pageSize, $total - $start)))
        foreach ($entry in $page) {
            if (($null -eq $entry.Date) -or ([int]$entry.Operation -ne 1)) {
                continue
            }
            $code = [int]$entry.ResultCode
            if (($code -ne 2) -and ($code -ne 3)) {
                continue
            }
            $when = ([DateTime]$entry.Date).ToLocalTime()
            if ($when -lt $since) {
                continue
            }
            $title = Get-Text $entry.Title
            $kb = [regex]::Match($title, 'KB\d{6,8}')
            if ($kb.Success -and ($skipped -contains $kb.Value.ToUpperInvariant())) {
                continue
            }
            if ($title.Length -gt 120) {
                $title = $title.Substring(0, 120)
            }
            $updates.Add([pscustomobject]@{ When = $when; Title = $title })
        }
        # Newest first: once a page reaches back past $days, the rest is older.
        if (($page.Count -eq 0) -or (@($page | Where-Object { ($null -ne $_.Date) -and (([DateTime]$_.Date).ToLocalTime() -lt $since) }).Count -gt 0)) {
            break
        }
    }
}
catch {
    Write-Verbose ('The update history could not be read: ' + $_.Exception.Message)
    $source = 'hotfixes'
    # Get-HotFix turns InstalledOn (text in more than one format) into a date.
    foreach ($fix in @(Get-HotFix -ErrorAction SilentlyContinue)) {
        $id = Get-Text $fix.HotFixID
        if (($id.Length -eq 0) -or (-not ($fix.InstalledOn -is [DateTime]))) {
            continue
        }
        $when = [DateTime]$fix.InstalledOn
        if ($when -lt $since) {
            continue
        }
        $updates.Add([pscustomobject]@{ When = $when; Title = $id })
    }
}

$kept = @($updates | Sort-Object -Property When -Descending | Select-Object -First $maxUpdates)
$sections = New-Object System.Collections.Generic.List[object]
foreach ($group in @($kept | Group-Object -Property { $_.When.ToString('yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture) } | Sort-Object -Property Name -Descending)) {
    $rows = New-Object System.Collections.Generic.List[object]
    foreach ($item in $group.Group) {
        $rows.Add([ordered]@{ id = 'update'; value = $item.Title })
    }
    $sections.Add([ordered]@{ id = 'day'; name = $group.Name; rows = $rows.ToArray() })
}

$result = 'none'
if ($kept.Count -gt 0) {
    $result = 'found'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{ days = $days; count = $kept.Count; source = $source }
    sections = $sections.ToArray()
}
