# Check: system.desktop-icons
# Are the desktop icons hidden? Right-click the desktop > View > "Show
# desktop icons" is HideIcons (DWORD) under the logged-in user's
# Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced: 1 hides every
# icon on the desktop (files too; they are still there). A policy can remove
# the desktop altogether: NoDesktop = 1 under
# Software\Microsoft\Windows\CurrentVersion\Policies\Explorer, the user's or
# the machine's (Group Policy "Hide and disable all items on the desktop").
# Read-only. Outputs one object: { result, facts }.
# Result codes (in this order):
#   policy  NoDesktop is on (a policy; not changed here)
#   hidden  HideIcons = 1 (fix: desktop.show-desktop-icons)
#   ok
# Facts: none.

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

$policy = 'Software\Microsoft\Windows\CurrentVersion\Policies\Explorer'
foreach ($root in @($UserHive, 'HKLM:')) {
    if ((Get-Dword ($root.TrimEnd('\') + '\' + $policy) 'NoDesktop') -eq 1) {
        [pscustomobject]@{ result = 'policy'; facts = [ordered]@{} }
        return
    }
}

$result = 'ok'
$advanced = $UserHive.TrimEnd('\') + '\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced'
if ((Get-Dword $advanced 'HideIcons') -eq 1) {
    $result = 'hidden'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{}
}
