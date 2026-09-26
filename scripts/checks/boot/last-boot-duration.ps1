# Check: boot.last-boot-duration
# How long did the last start-up take?
# Source: Microsoft-Windows-Diagnostics-Performance/Operational, event 100. Its
# EventData holds BootTime in milliseconds (MainPathBootTime + BootPostBootTime,
# i.e. power-on until the desktop is ready and idle).
# The first boot after installing updates is slow by design. When the newest
# event has BootIsRebootAfterInstall = true, the newest ordinary boot among the
# last 10 events is used instead (after_update tells which one was used).
# Read-only. Needs administrator rights (the log is restricted).
# Result codes: ok (60 s or less) / slow (more than 60 s) / no-data (the log does
# not exist, as on Windows Server, or has been disabled or emptied, as on some
# "optimized" systems).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$slowThresholdMs = 60000
$logName = 'Microsoft-Windows-Diagnostics-Performance/Operational'

function Read-XmlText {
    param([string]$Text)
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [System.Xml.XmlReader]::Create((New-Object System.IO.StringReader($Text)), $settings)
    try {
        $doc = New-Object System.Xml.XmlDocument
        $doc.Load($reader)
        # The unary comma keeps the pipeline from enumerating the document's nodes.
        return , $doc
    }
    finally {
        $reader.Close()
    }
}

function Get-EventDataMap {
    param($Record)
    $doc = Read-XmlText $Record.ToXml()
    $map = @{}
    foreach ($node in $doc.SelectNodes("//*[local-name()='EventData']/*[local-name()='Data']")) {
        $name = [string]$node.GetAttribute('Name')
        if ($name.Length -gt 0) {
            $map[$name] = ([string]$node.InnerText).Trim()
        }
    }
    return $map
}

$events = @()
try {
    $events = @(Get-WinEvent -FilterHashtable @{ LogName = $logName; Id = 100 } -MaxEvents 10 -ErrorAction Stop)
}
catch {
    # NoMatchingLogsFound: the log does not exist. NoMatchingEventsFound: no event 100.
    $errorId = [string]$_.FullyQualifiedErrorId
    if (($errorId -like 'NoMatchingLogsFound*') -or ($errorId -like 'NoMatchingEventsFound*')) {
        $events = @()
    }
    else {
        throw
    }
}
if ($events.Count -eq 0) {
    [pscustomobject]@{ result = 'no-data'; facts = [ordered]@{} }
    return
}

# Get-WinEvent returns the newest events first.
$chosen = $null
$chosenData = $null
foreach ($e in $events) {
    $data = Get-EventDataMap $e
    if (-not $data.ContainsKey('BootTime')) {
        continue
    }
    if ($null -eq $chosen) {
        $chosen = $e
        $chosenData = $data
    }
    if ($data['BootIsRebootAfterInstall'] -ne 'true') {
        $chosen = $e
        $chosenData = $data
        break
    }
}
if ($null -eq $chosen) {
    throw 'The boot performance events do not contain BootTime'
}

$invariant = [System.Globalization.CultureInfo]::InvariantCulture
$bootMs = [double]::Parse($chosenData['BootTime'], $invariant)

$facts = [ordered]@{
    boot_sec     = [math]::Round($bootMs / 1000, 1)
    boot_time    = $chosen.TimeCreated.ToString('o')
    after_update = ($chosenData['BootIsRebootAfterInstall'] -eq 'true')
}
$value = [double]0
if ($chosenData.ContainsKey('MainPathBootTime') -and [double]::TryParse($chosenData['MainPathBootTime'], [System.Globalization.NumberStyles]::Float, $invariant, [ref]$value)) {
    $facts['main_path_sec'] = [math]::Round($value / 1000, 1)
}
$value = [double]0
if ($chosenData.ContainsKey('BootPostBootTime') -and [double]::TryParse($chosenData['BootPostBootTime'], [System.Globalization.NumberStyles]::Float, $invariant, [ref]$value)) {
    $facts['post_boot_sec'] = [math]::Round($value / 1000, 1)
}

$result = 'ok'
if ($bootMs -gt $slowThresholdMs) {
    $result = 'slow'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
