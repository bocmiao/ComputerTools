# Check: system.pinyin-charset
# Does Microsoft Pinyin type traditional characters? Its settings live under
# the signed-in user's Software\Microsoft\InputMethod\Settings\CHS (read via
# -UserHive): "Output CharSet" 1 = traditional, 0 or missing = simplified
# (the default). Ctrl + Shift + F switches it, and people press that by
# accident ("EnableSimplifiedTraditionalOutputSwitch" 1 = the shortcut is on;
# missing = on, the default). Read-only.
# Result codes: simplified / traditional (fix: input.pinyin-simplified) /
# not-used (no Microsoft Pinyin settings: it was never used).
# Facts: shortcut (true when Ctrl + Shift + F is on).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$key = $UserHive.TrimEnd('\') + '\Software\Microsoft\InputMethod\Settings\CHS'

function Get-Number {
    param($Item, [string]$Name)
    if ($null -eq $Item) {
        return $null
    }
    $property = $Item.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    try {
        return [int]$property.Value
    }
    catch {
        return $null
    }
}

if (-not (Test-Path -LiteralPath $key)) {
    [pscustomobject]@{ result = 'not-used'; facts = [ordered]@{ shortcut = $true } }
    return
}
# A key without any values gives nothing here: everything is the default.
$item = Get-ItemProperty -LiteralPath $key
$charset = Get-Number $item 'Output CharSet'
$switch = Get-Number $item 'EnableSimplifiedTraditionalOutputSwitch'

$result = 'simplified'
if ($charset -eq 1) {
    $result = 'traditional'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ shortcut = ($switch -ne 0) }
}
