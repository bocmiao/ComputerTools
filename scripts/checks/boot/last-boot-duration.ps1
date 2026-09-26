# Check: boot.last-boot-duration
# How long did start-up take, from pressing the power button until the desktop
# appeared?
# Source: Microsoft-Windows-Diagnostics-Performance/Operational, event 100. Its
# EventData holds, in milliseconds:
#   MainPathBootTime  power-on until the desktop (or the sign-in screen) appears
#   BootPostBootTime  desktop until the system is idle (start-up apps, updates,
#                     virus scans...); reported as a fact only
#   BootTime          the sum of both
# The verdict uses MainPathBootTime (BootTime when an event lacks it).
# The first boot after installing updates is slow by design: the newest ordinary
# boot among the last 10 events is used. When all of them are first boots after
# installing updates (BootIsRebootAfterInstall = true), the newest one is
# reported as after-update instead of being judged.
# boot_when is the local time of the boot that was used ("yyyy-MM-dd HH:mm").
# Fast startup: HiberbootEnabled (HKLM\SYSTEM\CurrentControlSet\Control\
# Session Manager\Power; missing = 1, the default) and HibernateEnabled
# (HKLM\SYSTEM\CurrentControlSet\Control\Power; fast startup needs the
# hibernation file). Fact fast_startup: $false when either is 0, $true
# otherwise, left out when the keys cannot be read. Without fast startup every
# power-on is a full (cold) boot, which is slower, so a slow boot says so.
# Read-only. Needs administrator rights (the log is restricted).
# Result codes: ok (desktop within 60 s) / slow (more than 60 s) /
# slow-no-fast-startup (more than 60 s, fast startup off) / after-update /
# no-data (the log does not exist, as on Windows Server, or has been disabled or
# emptied, as on some "optimized" systems).

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

# $true / $false, or $null when it cannot be told.
function Get-FastStartup {
    try {
        $hiberboot = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power' -ErrorAction Stop
        $power = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Power' -ErrorAction Stop
    }
    catch {
        return $null
    }
    foreach ($pair in @(@($hiberboot, 'HiberbootEnabled'), @($power, 'HibernateEnabled'))) {
        $prop = $pair[0].PSObject.Properties[$pair[1]]
        if (($null -ne $prop) -and ([string]$prop.Value -eq '0')) {
            return $false
        }
    }
    return $true
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
$newestUpdate = $null
$newestUpdateData = $null
foreach ($e in $events) {
    $data = Get-EventDataMap $e
    if (-not $data.ContainsKey('BootTime')) {
        continue
    }
    if ($data['BootIsRebootAfterInstall'] -eq 'true') {
        if ($null -eq $newestUpdate) {
            $newestUpdate = $e
            $newestUpdateData = $data
        }
        continue
    }
    $chosen = $e
    $chosenData = $data
    break
}
$afterUpdateOnly = $false
if ($null -eq $chosen) {
    if ($null -eq $newestUpdate) {
        throw 'The boot performance events do not contain BootTime'
    }
    $chosen = $newestUpdate
    $chosenData = $newestUpdateData
    $afterUpdateOnly = $true
}

$invariant = [System.Globalization.CultureInfo]::InvariantCulture
$bootMs = [double]::Parse($chosenData['BootTime'], $invariant)
# Time until the desktop appears; the whole BootTime when the event lacks it.
$desktopMs = $bootMs
$value = [double]0
if ($chosenData.ContainsKey('MainPathBootTime') -and [double]::TryParse($chosenData['MainPathBootTime'], [System.Globalization.NumberStyles]::Float, $invariant, [ref]$value)) {
    $desktopMs = $value
}

$facts = [ordered]@{
    desktop_sec  = [math]::Round($desktopMs / 1000, 1)
    boot_sec     = [math]::Round($bootMs / 1000, 1)
    boot_when    = $chosen.TimeCreated.ToString('yyyy-MM-dd HH:mm', $invariant)
    boot_time    = $chosen.TimeCreated.ToString('o')
    after_update = ($chosenData['BootIsRebootAfterInstall'] -eq 'true')
}
$value = [double]0
if ($chosenData.ContainsKey('BootPostBootTime') -and [double]::TryParse($chosenData['BootPostBootTime'], [System.Globalization.NumberStyles]::Float, $invariant, [ref]$value)) {
    $facts['post_boot_sec'] = [math]::Round($value / 1000, 1)
}

$fastStartup = Get-FastStartup
if ($null -ne $fastStartup) {
    $facts['fast_startup'] = $fastStartup
}

if ($afterUpdateOnly) {
    $result = 'after-update'
}
elseif ($desktopMs -gt $slowThresholdMs) {
    $result = 'slow'
    if ($false -eq $fastStartup) {
        $result = 'slow-no-fast-startup'
    }
}
else {
    $result = 'ok'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
