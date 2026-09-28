# Check: system.windows-search
# The Windows Search service (WSearch) builds the search index, so that
# searching in the Start menu and File Explorer is fast and finds files by
# name and content. Windows starts it automatically (Automatic, Delayed
# Start). "Optimizing" tools and guides often disable it; then searching
# files is slow and the Start menu finds few of them (Microsoft, "Search
# indexing in Windows").
# Read-only. Outputs one object: { result, facts }.
# Result codes (in this order):
#   missing   there is no such service (Windows Server without the feature,
#             "Windows Search" turned off in Windows features, stripped
#             systems)
#   disabled  set to Disabled (fix: system.enable-windows-search)
#   manual    set to Manual: nothing starts it (same fix)
#   ok        starts automatically
# Facts: status (Running, Stopped, ...), start (Automatic, Manual, Disabled;
# delayed start shows as Automatic).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$service = Get-Service -Name 'WSearch' -ErrorAction SilentlyContinue
if ($null -eq $service) {
    [pscustomobject]@{ result = 'missing'; facts = [ordered]@{} }
    return
}

$start = [string]$service.StartType
$result = 'ok'
if ($start -eq 'Disabled') {
    $result = 'disabled'
}
elseif ($start -eq 'Manual') {
    $result = 'manual'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        status = [string]$service.Status
        start  = $start
    }
}
