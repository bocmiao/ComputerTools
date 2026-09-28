# Check: system.disabled-tools
# Are system tools turned off by policy? These are the values Group Policy
# writes for Microsoft's ADMX policies (all "User Configuration"); viruses and
# "lock" tools write them straight into the registry, often for the machine
# (HKLM) as well, so both the logged-in user's ($UserHive) and the machine's
# copy are read:
#   Software\Microsoft\Windows\CurrentVersion\Policies\System
#     DisableTaskMgr        "Remove Task Manager" (ADMX_CtrlAltDel)
#     DisableRegistryTools  "Prevent access to registry editing tools"
#   Software\Policies\Microsoft\Windows\System
#     DisableCMD            "Prevent access to the command prompt"
#   Software\Microsoft\Windows\CurrentVersion\Policies\Explorer
#     NoRun                 "Remove Run menu from Start Menu" (Win + R too)
#     NoControlPanel        "Prohibit access to Control Panel and PC settings"
#     NoFolderOptions       "Prevent access to the Folder Options"
#     NoViewContextMenu     "Remove File Explorer's default context menu"
#     NoClose               "Remove and prevent access to the Shut Down,
#                           Restart, Sleep, and Hibernate commands"
#     NoWinKeys             "Turn off Windows Key hotkeys"
#     DisallowRun           "Don't run specified Windows applications"
#     RestrictRun           "Run only specified Windows applications"
# A DWORD other than 0 counts. Read-only: policies are only reported, never
# changed here (plan, section 5).
# Result codes: disabled (advice) / ok.
# Facts: disabled (codes of the tools, in the order above), where ('user',
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

$systemKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\System'
$explorerKey = 'Software\Microsoft\Windows\CurrentVersion\Policies\Explorer'
$cmdKey = 'Software\Policies\Microsoft\Windows\System'

# Code, key, value name.
$policies = @(
    @('task-manager', $systemKey, 'DisableTaskMgr'),
    @('registry-editor', $systemKey, 'DisableRegistryTools'),
    @('cmd', $cmdKey, 'DisableCMD'),
    @('run', $explorerKey, 'NoRun'),
    @('control-panel', $explorerKey, 'NoControlPanel'),
    @('folder-options', $explorerKey, 'NoFolderOptions'),
    @('context-menu', $explorerKey, 'NoViewContextMenu'),
    @('shutdown', $explorerKey, 'NoClose'),
    @('win-keys', $explorerKey, 'NoWinKeys'),
    @('disallow-run', $explorerKey, 'DisallowRun'),
    @('restrict-run', $explorerKey, 'RestrictRun')
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
$disabled = New-Object System.Collections.Generic.List[string]
$values = New-Object System.Collections.Generic.List[string]
$inUser = $false
$inMachine = $false
foreach ($policy in $policies) {
    $found = $false
    foreach ($root in $roots) {
        $value = Get-Dword ($root[1] + '\' + $policy[1]) $policy[2]
        if (($null -eq $value) -or ($value -eq 0)) {
            continue
        }
        $found = $true
        $values.Add($root[0] + '\' + $policy[1] + '\' + $policy[2])
        if ($root[0] -eq 'HKCU') {
            $inUser = $true
        }
        else {
            $inMachine = $true
        }
    }
    if ($found) {
        $disabled.Add($policy[0])
    }
}

$facts = [ordered]@{ disabled = $disabled.ToArray() }
$result = 'ok'
if ($disabled.Count -gt 0) {
    $result = 'disabled'
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
