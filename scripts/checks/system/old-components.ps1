# Check: system.old-components
# Two Windows components that stay off until a program asks for them:
# .NET Framework 3.5 (with 2.0 and 3.0; older programs) and DirectPlay (older
# games). Both are Windows features, read from Win32_OptionalFeature
# (InstallState 1 enabled, 2 disabled, 3 absent). From Windows 11 26H1
# (build 28000) .NET Framework 3.5 is a standalone installer instead of a
# feature; it records itself where .NET Framework 1.0 to 3.5 always did
# (Microsoft, "Determine which .NET Framework versions are installed":
# HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v3.5, Install = 1), which
# is read when the NetFx3 feature is not on.
# Neither being off is a problem: the results say what each is for and link
# the tool that turns it on. Read-only.
# Result codes: ok (both on) / netfx3-off / directplay-off / both-off /
# unsupported (Windows Server: Server Manager adds them there).
# Facts: netfx3, directplay (on / off).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$facts = [ordered]@{ netfx3 = ''; directplay = '' }

$os = Get-CimInstance -ClassName Win32_OperatingSystem -Property ProductType
if ([int]$os.ProductType -ne 1) {
    [pscustomobject]@{ result = 'unsupported'; facts = $facts }
    return
}

$states = @{}
foreach ($feature in @(Get-CimInstance -ClassName Win32_OptionalFeature -Filter "Name = 'NetFx3' OR Name = 'DirectPlay'")) {
    $states[[string]$feature.Name] = [int]$feature.InstallState
}

function Get-OnOff {
    param($State)
    if ($State -eq 1) {
        return 'on'
    }
    return 'off'
}

$facts.netfx3 = Get-OnOff $states['NetFx3']
if ($facts.netfx3 -eq 'off') {
    $install = $null
    try {
        $install = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\NET Framework Setup\NDP\v3.5' -ErrorAction Stop).PSObject.Properties['Install']
    }
    catch {
        Write-Verbose 'no .NET Framework 3.5 entry'
    }
    if (($null -ne $install) -and ([string]$install.Value -eq '1')) {
        $facts.netfx3 = 'on'
    }
}
$facts.directplay = Get-OnOff $states['DirectPlay']

$result = 'ok'
if (($facts.netfx3 -eq 'off') -and ($facts.directplay -eq 'off')) {
    $result = 'both-off'
}
elseif ($facts.netfx3 -eq 'off') {
    $result = 'netfx3-off'
}
elseif ($facts.directplay -eq 'off') {
    $result = 'directplay-off'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
