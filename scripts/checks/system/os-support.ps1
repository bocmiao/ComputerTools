# Check: system.os-support
# Is this Windows version still supported by Microsoft?
# Reads HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion (CurrentBuild,
# DisplayVersion, EditionID, ProductName, InstallationType). ProductName still
# says "Windows 10" on Windows 11, so the OS is decided by build >= 22000.
# Read-only. Result codes:
#   supported       Windows 11 within its servicing period
#   ending-soon     Windows 11, but support ends within 60 days
#                   ($endingSoonDays): install the latest feature update now
#   ltsc            long-term servicing edition within its servicing period
#   ltsc-ending-soon  long-term servicing edition ending within 60 days
#   win10           Windows 10 22H2, Home/Pro family: can enroll in consumer ESU
#   expired         older Windows 10, Home/Pro family: update to 22H2, then ESU
#   expired-win11   Windows 11 version past its end date: install the latest
#                   feature update
#   expired-no-esu  past its end and no consumer ESU: Enterprise / Education /
#                   IoT editions of Windows 10, expired LTSB/LTSC, and any
#                   Windows 10 once consumer ESU has ended
#
# End-of-servicing dates come from the Microsoft Lifecycle pages (checked on
# 2026-09-26). The pages show dates in Pacific Time; the times they list
# ("10/14/2026 6:59:59 AM") are UTC, so the last supported day is one day
# earlier. Update this table when Microsoft publishes a new version: an unknown
# build throws, and the engine shows "unknown" instead of guessing.
#   https://learn.microsoft.com/lifecycle/products/windows-11-home-and-pro
#   https://learn.microsoft.com/lifecycle/products/windows-11-enterprise-and-education
#   https://learn.microsoft.com/lifecycle/products/windows-10-home-and-pro
#   https://www.microsoft.com/windows/extended-security-updates (consumer ESU)
#   LTSB/LTSC: windows-10-2015-ltsb, windows-10-2016-ltsb,
#   windows-10-enterprise-ltsc-2019, windows-10-enterprise-ltsc-2021,
#   windows-10-iot-enterprise-ltsc-2021, windows-11-enterprise-ltsc-2024,
#   windows-11-iot-enterprise-ltsc-2024 (and the IoT 2015/2016/2019 pages)
# Not in the table yet: Windows 11 26H2 (build 26300) was in the Release Preview
# channel in September 2026 without published dates.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# Windows 11 feature updates: build = version, Home/Pro end, Enterprise/Education end.
# Home/Pro also covers Pro Education, Pro for Workstations and SE.
$win11 = @{
    22000 = @('21H2', '2023-10-10', '2024-10-08')
    22621 = @('22H2', '2024-10-08', '2025-10-14')
    22631 = @('23H2', '2025-11-11', '2026-11-10')
    26100 = @('24H2', '2026-10-13', '2027-10-12')
    26200 = @('25H2', '2027-10-12', '2028-10-10')
    28000 = @('26H1', '2028-03-14', '2029-03-13')
}

# Long-term servicing: build = version, Enterprise LTSB/LTSC end, IoT Enterprise LTSB/LTSC end.
$ltscTable = @{
    10240 = @('LTSB 2015', '2025-10-14', '2025-10-14')
    14393 = @('LTSB 2016', '2026-10-13', '2026-10-13')
    17763 = @('LTSC 2019', '2029-01-09', '2029-01-09')
    19044 = @('LTSC 2021', '2027-01-12', '2032-01-13')
    26100 = @('LTSC 2024', '2029-10-09', '2034-10-10')
}

# Windows 10 22H2 (build 19045) is the last Windows 10 release. Home/Pro support
# ended on 2025-10-14; consumer Extended Security Updates (Home, Pro, Pro Education,
# Workstations, version 22H2 only) run until 2027-10-12.
$win10FinalBuild = 19045
$win10EndDate = '2025-10-14'
$esuEndDate = '2027-10-12'

# Support that ends within this many days is advice, not "ok": the user still
# has time to install the feature update (or move off an ending LTSC).
$endingSoonDays = 60

function Get-PropertyText {
    param($Object, [string]$Name)
    if ($null -eq $Object) {
        return ''
    }
    $prop = $Object.PSObject.Properties[$Name]
    if (($null -eq $prop) -or ($null -eq $prop.Value)) {
        return ''
    }
    return ([string]$prop.Value).Trim()
}

$today = (Get-Date).Date

function ConvertTo-Date {
    param([string]$Date)
    return [datetime]::ParseExact($Date, 'yyyy-MM-dd', [System.Globalization.CultureInfo]::InvariantCulture)
}

function Test-DatePassed {
    param([string]$Date)
    return ($today -gt (ConvertTo-Date $Date))
}

# Supported days left, today and the last day included (1 on the last day).
function Get-DaysLeft {
    param([string]$Date)
    return [int](((ConvertTo-Date $Date) - $today).TotalDays) + 1
}

$cv = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'

$buildText = Get-PropertyText $cv 'CurrentBuild'
if ($buildText.Length -eq 0) {
    $buildText = Get-PropertyText $cv 'CurrentBuildNumber'
}
$build = 0
if (-not [int]::TryParse($buildText, [ref]$build)) {
    throw ("Cannot read the Windows build number: '{0}'" -f $buildText)
}

$displayVersion = Get-PropertyText $cv 'DisplayVersion'
if ($displayVersion.Length -eq 0) {
    $displayVersion = Get-PropertyText $cv 'ReleaseId'
}
$edition = Get-PropertyText $cv 'EditionID'
$productName = Get-PropertyText $cv 'ProductName'
$installationType = Get-PropertyText $cv 'InstallationType'

if ($installationType -eq 'Server') {
    throw ('Windows Server is not covered by this check: {0}' -f $productName)
}

# LTSB/LTSC editions: EnterpriseS, EnterpriseSN, IoTEnterpriseS, IoTEnterpriseSK, ...
$isLtsc = $edition -match '^(IoT)?EnterpriseS'
$isIot = $edition -like 'IoT*'
# Enterprise and Education follow the longer lifecycle; everything else (Core*,
# Professional*, CloudEdition*) follows Home/Pro.
$isEnterprise = $edition -match '^(Enterprise|Education|IoTEnterprise|ServerRdsh)'

$os = 'Windows 10'
if ($build -ge 22000) {
    $os = 'Windows 11'
}

$version = $displayVersion
$endDate = ''
$result = $null

$esuOpen = -not (Test-DatePassed $esuEndDate)

if ($isLtsc) {
    if (-not $ltscTable.ContainsKey($build)) {
        throw ('Unknown LTSB/LTSC build {0} ({1}); the lifecycle table needs an update' -f $build, $edition)
    }
    $row = $ltscTable[$build]
    $version = $row[0]
    $endDate = $row[1]
    if ($isIot) {
        $endDate = $row[2]
    }
    if (Test-DatePassed $endDate) {
        $result = 'expired-no-esu'
    }
    elseif ((Get-DaysLeft $endDate) -le $endingSoonDays) {
        $result = 'ltsc-ending-soon'
    }
    else {
        $result = 'ltsc'
    }
}
elseif ($build -ge 22000) {
    if (-not $win11.ContainsKey($build)) {
        throw ('Unknown Windows 11 build {0} (DisplayVersion {1}); the lifecycle table needs an update' -f $build, $displayVersion)
    }
    $row = $win11[$build]
    $version = $row[0]
    $endDate = $row[1]
    if ($isEnterprise) {
        $endDate = $row[2]
    }
    if (Test-DatePassed $endDate) {
        $result = 'expired-win11'
    }
    elseif ((Get-DaysLeft $endDate) -le $endingSoonDays) {
        $result = 'ending-soon'
    }
    else {
        $result = 'supported'
    }
}
elseif ($build -eq $win10FinalBuild) {
    # Every Windows 10 22H2 edition is past its end of servicing (Home/Pro and
    # Enterprise/Education both ended on 2025-10-14). Only Home/Pro-family
    # editions can enroll in consumer ESU from Settings.
    $version = '22H2'
    $endDate = $win10EndDate
    if ((-not $isEnterprise) -and $esuOpen) {
        $result = 'win10'
    }
    else {
        $result = 'expired-no-esu'
    }
}
elseif ($build -ge 10240) {
    # Older Windows 10 releases (non-LTSC) all ended before 22H2 did. Home/Pro
    # can still update to 22H2 and enroll in consumer ESU while it lasts.
    if ((-not $isEnterprise) -and $esuOpen) {
        $result = 'expired'
    }
    else {
        $result = 'expired-no-esu'
    }
}
else {
    throw ('Windows build {0} is older than Windows 10' -f $build)
}

$facts = [ordered]@{
    os      = $os
    version = $version
    build   = $build
    edition = $edition
}
if ($endDate.Length -gt 0) {
    $facts['end_date'] = $endDate
}
if (($result -eq 'ending-soon') -or ($result -eq 'ltsc-ending-soon')) {
    $facts['days_left'] = Get-DaysLeft $endDate
}
if (($result -eq 'win10') -or ($result -eq 'expired')) {
    $facts['esu_end'] = $esuEndDate
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
