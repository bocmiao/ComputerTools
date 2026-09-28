# Check: system.start-menu
# Is the Start menu there for the logged-in user, and have the Start menu,
# search, the notification area or Explorer (the taskbar) crashed or hung
# lately? The steps of Microsoft's "Troubleshoot Start menu errors":
# - The Start menu is a system app, Microsoft.Windows.StartMenuExperienceHost
#   (Windows 10 1809 and earlier: Microsoft.Windows.ShellExperienceHost), that
#   must be registered for each user. Asked with Get-AppxPackage for the
#   logged-in user (the SID in -UserHive; asking for another user needs
#   administrator rights, without them the current user is asked). When it is
#   not registered, its files tell what can be done:
#   %windir%\SystemApps\<package>_cw5n1h2txyewy\AppxManifest.xml is there
#   (not-registered: it can be registered again for the user) or not
#   (missing: Microsoft has no supported way to install it again; roll back
#   the update, restore or reset). When Get-AppxPackage cannot be asked at
#   all, only the crashes are looked at.
# - Crashes and hangs: Application Error 1000 and Application Hang 1002 in the
#   Application log in the last $days days, for the processes in $processes
#   (the first value of both events is the program's file name).
# Read-only.
# Result codes: ok / crashing (advice) / not-registered, missing (problem) /
# unsupported (Server Core: no Start menu).
# Facts: days, crashes, hangs, apps (codes of the parts that crashed or hung:
# start, search, shell, explorer, in that order).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$days = 7
$maxEvents = 2000
$ntKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'

# Program file name (lower case) and the part of Windows it shows.
$processes = @{
    'startmenuexperiencehost.exe' = 'start'
    'searchhost.exe'              = 'search'
    'searchapp.exe'               = 'search'
    'searchui.exe'                = 'search'
    'shellexperiencehost.exe'     = 'shell'
    'explorer.exe'                = 'explorer'
}
$order = @('start', 'search', 'shell', 'explorer')

function Get-Value {
    param([string]$Path, [string]$Name)
    try {
        return (Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop).$Name
    }
    catch {
        return $null
    }
}

# $true / $false, or $null when Get-AppxPackage cannot be asked.
function Test-Registered {
    param([string]$Name, [string]$Sid)
    try {
        if ($Sid.Length -gt 0) {
            return (@(Get-AppxPackage -User $Sid -Name $Name -ErrorAction Stop).Count -gt 0)
        }
        return (@(Get-AppxPackage -Name $Name -ErrorAction Stop).Count -gt 0)
    }
    catch {
        Write-Verbose ('Get-AppxPackage failed: {0}' -f $_.Exception.Message)
    }
    if ($Sid.Length -gt 0) {
        try {
            return (@(Get-AppxPackage -Name $Name -ErrorAction Stop).Count -gt 0)
        }
        catch {
            Write-Verbose ('Get-AppxPackage failed: {0}' -f $_.Exception.Message)
        }
    }
    return $null
}

function Get-AppEvent {
    param([hashtable]$Filter)
    try {
        return @(Get-WinEvent -FilterHashtable $Filter -MaxEvents $maxEvents -ErrorAction Stop)
    }
    catch {
        # Get-WinEvent reports "no events" as an error; that simply means none.
        if ([string]$_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {
            return @()
        }
        throw
    }
}

if ([string](Get-Value $ntKey 'InstallationType') -eq 'Server Core') {
    [pscustomobject]@{ result = 'unsupported'; facts = [ordered]@{ days = $days } }
    return
}

$sid = ''
if ($UserHive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
    $sid = $Matches[1]
}
$build = 0
if (-not [int]::TryParse([string](Get-Value $ntKey 'CurrentBuild'), [ref]$build)) {
    $build = 0
}
$package = 'Microsoft.Windows.StartMenuExperienceHost'
if (($build -gt 0) -and ($build -lt 18362)) {
    $package = 'Microsoft.Windows.ShellExperienceHost'
}
$registered = Test-Registered $package $sid

# Crashes and hangs of the Start menu, search, the notification area and
# Explorer.
$since = (Get-Date).AddDays(-$days)
$seen = @{}
$crashes = 0
$hangs = 0
foreach ($record in (Get-AppEvent @{ LogName = 'Application'; Id = 1000, 1002; StartTime = $since })) {
    $provider = [string]$record.ProviderName
    if (($provider -ne 'Application Error') -and ($provider -ne 'Application Hang')) {
        continue
    }
    $properties = @($record.Properties)
    if ($properties.Count -eq 0) {
        continue
    }
    $name = ((([string]$properties[0].Value) -split '\\')[-1]).Trim().ToLowerInvariant()
    if (-not $processes.ContainsKey($name)) {
        continue
    }
    $seen[$processes[$name]] = $true
    if ($provider -eq 'Application Error') {
        $crashes++
    }
    else {
        $hangs++
    }
}
$apps = @($order | Where-Object { $seen.ContainsKey($_) })

$result = 'ok'
if ($registered -eq $false) {
    $manifest = Join-Path $env:windir ('SystemApps\' + $package + '_cw5n1h2txyewy\AppxManifest.xml')
    $result = 'missing'
    if (Test-Path -LiteralPath $manifest) {
        $result = 'not-registered'
    }
}
elseif (($crashes + $hangs) -gt 0) {
    $result = 'crashing'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        days    = $days
        crashes = $crashes
        hangs   = $hangs
        apps    = $apps
    }
}
