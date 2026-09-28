# Check: system.shortcut-flag
# Is the IsShortcut value gone from the shortcut file types? A popular tweak
# to remove the little arrow on shortcut icons deletes it from
# HKLM\SOFTWARE\Classes\lnkfile (and piffile). Windows then no longer treats
# shortcuts as shortcuts: "Manage" in the context menu of This PC says the
# file has no associated program, taskbar pins and the Win + X menu stop
# working, shortcuts cannot be pinned. IsShortcut is an empty REG_SZ that
# Windows has by default.
# Read-only. Result codes: missing (HKLM lnkfile has no IsShortcut; fixed by
# system.restore-shortcut-flag) / ok.
# Facts: lnkfile, piffile (true when IsShortcut is there), user_lnkfile: a
# per-user HKCU\Software\Classes\lnkfile (rare; -UserHive) overrides the
# machine key for that user, true / false when it exists, '' when it does
# not. It is only reported: creating that key would hide the machine one.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

# $true when the key has the value, $false when it has not, $null when the
# key is not there.
function Test-Value {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -ErrorAction Stop
    }
    catch {
        return $null
    }
    return ($null -ne $item.PSObject.Properties[$Name])
}

$lnk = Test-Value 'HKLM:\SOFTWARE\Classes\lnkfile' 'IsShortcut'
$pif = Test-Value 'HKLM:\SOFTWARE\Classes\piffile' 'IsShortcut'
$userLnk = Test-Value ($UserHive.TrimEnd('\') + '\Software\Classes\lnkfile') 'IsShortcut'

$facts = [ordered]@{ lnkfile = ($lnk -eq $true); piffile = ($pif -eq $true); user_lnkfile = '' }
if ($null -ne $userLnk) {
    $facts.user_lnkfile = $userLnk
}

$result = 'ok'
if ($lnk -ne $true) {
    $result = 'missing'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
