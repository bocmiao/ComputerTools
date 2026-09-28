# Check: system.taskbar-policies
# Are the taskbar, the notification area (the icons at the bottom right) or
# the Start menu taken away or locked by policy? These are the values Group
# Policy writes for Microsoft's ADMX policies ("Start Menu and Taskbar":
# StartMenu.admx and Taskbar.admx); "optimizer" tools, tutorials and viruses
# write them straight into the registry, sometimes for the machine (HKLM) as
# well, so both the logged-in user's ($UserHive) and the machine's copy are
# read:
#   Software\Microsoft\Windows\CurrentVersion\Policies\Explorer
#     NoTrayItemsDisplay      "Hide the notification area"
#     HideSCAVolume           "Remove the volume control icon"
#     HideSCANetwork          "Remove the networking icon"
#     HideSCAPower            "Remove the battery meter"
#     HideClock               "Remove Clock from the system notification area"
#     NoTrayContextMenu       "Remove access to the context menus for the
#                             taskbar"
#     NoSetTaskbar            "Prevent changes to Taskbar and Start Menu
#                             Settings"
#     LockTaskbar             "Lock the Taskbar"
#     TaskbarLockAll          "Lock all taskbar settings"
#     NoStartMenuMorePrograms "Remove All Programs list from the Start menu"
#     NoChangeStartMenu       "Prevent users from customizing their Start
#                             Screen"
#   Software\Policies\Microsoft\Windows\Explorer
#     DisableNotificationCenter "Remove Notifications and Action Center"
#     TaskbarNoPinnedList     "Remove pinned programs from the Taskbar"
#     NoPinningToTaskbar      "Do not allow pinning programs to the Taskbar"
#     LockedStartLayout       "Start Layout" (a fixed Start layout)
# A DWORD other than 0 counts. The Win key, "Run" and shut down policies are
# read by system.disabled-tools. Read-only: policies are only reported, never
# changed here (plan, section 5).
# Result codes: set (advice) / ok.
# Facts: items (codes, in the order of $policies, each once), where ('user',
# 'machine' or 'both'), values (the values that are set, "HKCU\...\Name";
# never the user's SID).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

$explorerKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\Explorer'
$policyKey = 'Software\Policies\Microsoft\Windows\Explorer'

# Code, key, value name. Codes that come from more than one value are listed
# once in the facts.
$policies = @(
    @('tray-all', $explorerKey, 'NoTrayItemsDisplay'),
    @('volume', $explorerKey, 'HideSCAVolume'),
    @('network', $explorerKey, 'HideSCANetwork'),
    @('battery', $explorerKey, 'HideSCAPower'),
    @('clock', $explorerKey, 'HideClock'),
    @('notification-center', $policyKey, 'DisableNotificationCenter'),
    @('taskbar-menu', $explorerKey, 'NoTrayContextMenu'),
    @('taskbar-settings', $explorerKey, 'NoSetTaskbar'),
    @('taskbar-locked', $explorerKey, 'LockTaskbar'),
    @('taskbar-locked', $explorerKey, 'TaskbarLockAll'),
    @('pinned-removed', $policyKey, 'TaskbarNoPinnedList'),
    @('no-pinning', $policyKey, 'NoPinningToTaskbar'),
    @('all-apps', $explorerKey, 'NoStartMenuMorePrograms'),
    @('start-locked', $explorerKey, 'NoChangeStartMenu'),
    @('start-locked', $policyKey, 'LockedStartLayout')
)

# A DWORD value, or $null when the key or the value is not there (or it is not
# a DWORD).
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

$roots = @(
    @('HKCU', $UserHive.TrimEnd('\')),
    @('HKLM', 'HKLM:')
)
$items = New-Object System.Collections.Generic.List[string]
$values = New-Object System.Collections.Generic.List[string]
$inUser = $false
$inMachine = $false
foreach ($policy in $policies) {
    foreach ($root in $roots) {
        $value = Get-Dword ($root[1] + '\' + $policy[1]) $policy[2]
        if (($null -eq $value) -or ($value -eq 0)) {
            continue
        }
        $values.Add($root[0] + '\' + $policy[1] + '\' + $policy[2])
        if ($root[0] -eq 'HKCU') {
            $inUser = $true
        }
        else {
            $inMachine = $true
        }
        if (-not $items.Contains($policy[0])) {
            $items.Add($policy[0])
        }
    }
}

$facts = [ordered]@{ items = $items.ToArray() }
$result = 'ok'
if ($items.Count -gt 0) {
    $result = 'set'
    $where = 'user'
    if ($inUser -and $inMachine) {
        $where = 'both'
    }
    elseif ($inMachine) {
        $where = 'machine'
    }
    $facts['where'] = $where
    $facts['values'] = ($values -join '; ')
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
