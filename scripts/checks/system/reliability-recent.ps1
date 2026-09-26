# Check: system.reliability-recent
# Program crashes, hangs and blue screens over the last 7 days, from the
# reliability history (Win32_ReliabilityRecords, the data behind Reliability
# Monitor):
#   SourceName "Application Error", EventIdentifier 1000       -> program crashed
#   SourceName "Application Hang",  EventIdentifier 1002       -> program stopped responding
#   SourceName "Microsoft-Windows-WER-SystemErrorReporting", 1001 -> blue screen (bugcheck)
#   SourceName "EventLog", EventIdentifier 6008                -> unexpected shutdown (fact only)
# SourceName is the event provider name, which is not localized.
# Read-only. Result codes: quiet / some.
#
# "some" means at least one blue screen, or at least 3 crashes/hangs in 7 days.
# A single crash now and then is normal and is not reported (plan: do not create
# anxiety).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 7
$appProblemThreshold = 3

# ManagementDateTimeConverter lives in System.Management, which Windows PowerShell
# does not always load before the first WMI call.
Add-Type -AssemblyName System.Management
$since = (Get-Date).AddDays(-$days)
$dmtf = [System.Management.ManagementDateTimeConverter]::ToDmtfDateTime($since)

$records = @(Get-CimInstance -ClassName Win32_ReliabilityRecords -Filter ("TimeGenerated >= '{0}'" -f $dmtf))

$crashes = 0
$hangs = 0
$bluescreens = 0
$unexpected = 0
$apps = @{}

foreach ($r in $records) {
    $source = [string]$r.SourceName
    $id = [long]$r.EventIdentifier
    $isApp = $false

    if (($source -eq 'Application Error') -and ($id -eq 1000)) {
        $crashes++
        $isApp = $true
    }
    elseif (($source -eq 'Application Hang') -and ($id -eq 1002)) {
        $hangs++
        $isApp = $true
    }
    elseif (($source -eq 'Microsoft-Windows-WER-SystemErrorReporting') -and ($id -eq 1001)) {
        $bluescreens++
    }
    elseif (($source -eq 'EventLog') -and ($id -eq 6008)) {
        $unexpected++
    }

    if ($isApp) {
        # ProductName is what Reliability Monitor shows as the source; fall back to
        # the first insertion string (the executable name).
        $app = ([string]$r.ProductName).Trim()
        if ($app.Length -eq 0) {
            $strings = @($r.InsertionStrings)
            if ($strings.Count -gt 0) {
                $app = ([string]$strings[0]).Trim()
            }
        }
        if ($app.Length -gt 0) {
            if ($apps.ContainsKey($app)) {
                $apps[$app] = $apps[$app] + 1
            }
            else {
                $apps[$app] = 1
            }
        }
    }
}

$facts = [ordered]@{
    days                 = $days
    app_crashes          = $crashes
    app_hangs            = $hangs
    bluescreens          = $bluescreens
    unexpected_shutdowns = $unexpected
}

if ($apps.Count -gt 0) {
    $top = $apps.GetEnumerator() | Sort-Object -Property @{ Expression = { $_.Value }; Descending = $true }, @{ Expression = { $_.Key }; Descending = $false } | Select-Object -First 1
    $facts['top_app'] = [string]$top.Key
    $facts['top_app_count'] = [int]$top.Value
}

$result = 'quiet'
if (($bluescreens -gt 0) -or (($crashes + $hangs) -ge $appProblemThreshold)) {
    $result = 'some'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
