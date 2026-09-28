# Check: system.windows-ads
# Which of Windows' own suggestions and promotions are on for the logged-in
# user. The groups are the settings of the features that turn them off; a
# group is on when one of its DWORD values is missing (Windows' default) or
# not 0, like those features read them:
#   tips       ContentDeliveryManager SubscribedContent-338389Enabled,
#              SoftLandingEnabled, SubscribedContent-310093Enabled and
#              UserProfileEngagement ScoobeSystemSettingEnabled (ads.tips-off)
#   lockscreen ContentDeliveryManager RotatingLockScreenOverlayEnabled,
#              SubscribedContent-338387Enabled (ads.lockscreen-tips-off)
#   settings   ContentDeliveryManager SubscribedContent-338393Enabled,
#              -353694Enabled, -353696Enabled (ads.settings-suggestions-off)
#   installs   ContentDeliveryManager SilentInstalledAppsEnabled
#              (ads.silent-app-installs-off)
#   start      Explorer\Advanced Start_IrisRecommendations, Windows 11 only
#              (ads.start-recommendations-off)
#   sync       Explorer\Advanced ShowSyncProviderNotifications
#              (explorer.hide-sync-ads)
# Read-only. Result codes: on (at least one group is on) / off.
# Facts: count (groups on), and one true/false fact per group (start is left
# out before Windows 11).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

# A DWORD value, or $null when the key or the value is not there.
function Get-Dword {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop
    }
    catch {
        return $null
    }
    $value = $item.$Name
    if ($value -is [int]) {
        return $value
    }
    return $null
}

# True when one of the values is missing or not 0.
function Test-AnyOn {
    param([string]$Path, [string[]]$Names)
    foreach ($name in $Names) {
        if ((Get-Dword $Path $name) -ne 0) {
            return $true
        }
    }
    return $false
}

function Get-Build {
    $key = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    foreach ($name in @('CurrentBuild', 'CurrentBuildNumber')) {
        try {
            $text = [string](Get-ItemProperty -LiteralPath $key -Name $name -ErrorAction Stop).$name
        }
        catch {
            continue
        }
        $number = 0
        if ([int]::TryParse($text, [ref]$number)) {
            return $number
        }
    }
    return 0
}

$root = $UserHive.TrimEnd('\') + '\Software\Microsoft\Windows\CurrentVersion'
$delivery = $root + '\ContentDeliveryManager'
$advanced = $root + '\Explorer\Advanced'

$groups = [ordered]@{}
$groups['tips'] = (Test-AnyOn $delivery @('SubscribedContent-338389Enabled', 'SoftLandingEnabled', 'SubscribedContent-310093Enabled')) -or
    (Test-AnyOn ($root + '\UserProfileEngagement') @('ScoobeSystemSettingEnabled'))
$groups['lockscreen'] = Test-AnyOn $delivery @('RotatingLockScreenOverlayEnabled', 'SubscribedContent-338387Enabled')
$groups['settings'] = Test-AnyOn $delivery @('SubscribedContent-338393Enabled', 'SubscribedContent-353694Enabled', 'SubscribedContent-353696Enabled')
$groups['installs'] = Test-AnyOn $delivery @('SilentInstalledAppsEnabled')
if ((Get-Build) -ge 22000) {
    $groups['start'] = Test-AnyOn $advanced @('Start_IrisRecommendations')
}
$groups['sync'] = Test-AnyOn $advanced @('ShowSyncProviderNotifications')

$count = @($groups.Values | Where-Object { $_ }).Count
$facts = [ordered]@{ count = $count }
foreach ($name in $groups.Keys) {
    $facts[$name] = [bool]$groups[$name]
}

[pscustomobject]@{
    result = $(if ($count -gt 0) { 'on' } else { 'off' })
    facts  = $facts
}
