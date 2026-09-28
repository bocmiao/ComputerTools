# Check: system.temp-profile
# Is the logged-in user on a temporary profile? When Windows cannot load a
# user's profile it signs them in with a temporary one ("You've been signed in
# with a temporary profile"): the desktop and documents look empty, and what
# is saved there is deleted at sign-out. The user's own files are usually
# still in their profile folder.
# Signs, read-only:
#   - the profile folder the session really uses: USERPROFILE under the
#     user's "Volatile Environment" (set at sign-in) is a folder named TEMP
#     or TEMP.<something>;
#   - ProfileList has a <SID>.bak key next to the user's (Windows renamed
#     the entry it could not load; the usual fix renames it back);
#   - User Profile Service event 1511 ("cannot find the local profile and is
#     logging you on with a temporary profile") in the last 30 days.
# Only yes/no and counts are output: no SID, no user name, no path.
# Result codes: temp (this session is on a temporary profile), ok.
# Facts: bak (true/false), events (count of 1511 in 30 days, -1 when the log
# cannot be read).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

# The user's SID: from the hive path the engine passes (HKEY_USERS\<SID>),
# or this process's user when it is HKCU.
$sid = ''
if ($UserHive -match 'HKEY_USERS\\(S-1-5-[0-9-]+)\\?$') {
    $sid = $Matches[1]
}
else {
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
}

$profilePath = ''
try {
    $volatile = Get-ItemProperty -LiteralPath ($UserHive.TrimEnd('\') + '\Volatile Environment') -Name 'USERPROFILE' -ErrorAction Stop
    $profilePath = [string]$volatile.USERPROFILE
}
catch {
    $profilePath = ''
}
$temp = $false
if ($profilePath.Length -gt 0) {
    $leaf = Split-Path -Path $profilePath.TrimEnd('\') -Leaf
    $temp = ($leaf -match '^TEMP(\..+)?$')
}

$bak = $false
if ($sid.Length -gt 0) {
    $bak = Test-Path -LiteralPath ('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\' + $sid + '.bak')
}

# Event 1511 of the User Profile Service. Filter by log, ID and time only and
# compare the provider here (a ProviderName filter can fail on some systems).
$events = -1
try {
    $since = (Get-Date).AddDays(-30)
    $found = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; Id = 1511; StartTime = $since } -ErrorAction Stop |
            Where-Object { $_.ProviderName -eq 'Microsoft-Windows-User Profiles Service' })
    $events = $found.Count
}
catch {
    # No matching events is an error for Get-WinEvent too.
    if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {
        $events = 0
    }
}

$result = 'ok'
if ($temp) {
    $result = 'temp'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        bak    = $bak
        events = $events
    }
}
