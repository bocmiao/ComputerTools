# Check: display.colors
# Did the screen turn black and white (or inverted, or oddly colored) because
# of an accessibility setting? Two common accidents:
# - Color filters, HKCU\Software\Microsoft\ColorFiltering: Active (DWORD, 1 =
#   on), FilterType (0 grayscale, 1 inverted, 2 grayscale inverted,
#   3 deuteranopia, 4 protanopia, 5 tritanopia), HotkeyEnabled (1 = Windows
#   logo key + Ctrl + C switches them on and off). A registry change is used
#   after the next sign-in; the shortcut and Settings work at once.
# - Contrast themes (high contrast): HKCU\Control Panel\Accessibility\
#   HighContrast, Flags (a number as text) with bit 0x1 (HCF_HIGHCONTRASTON)
#   set; left Alt + left Shift + Print Screen switches them when bit 0x4
#   (HCF_HOTKEYACTIVE) is set.
# Read-only; reads the signed-in user's hive (-UserHive).
# Result codes, in this order: grayscale (filter 0 or 2) / inverted (1) /
# color-filter (3-5) / high-contrast / ok.
# Facts: filter_on, filter_type, filter_hotkey, high_contrast,
# contrast_hotkey ('' when not set).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$root = $UserHive.TrimEnd('\')

# A value as a whole number, or $null when it is missing or not a number.
function Get-Number {
    param([string]$Path, [string]$Name)
    try {
        $value = (Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop).$Name
    }
    catch {
        return $null
    }
    $n = [long]0
    if (($null -ne $value) -and [long]::TryParse(([string]$value).Trim(), [ref]$n)) {
        return $n
    }
    return $null
}

$filterKey = $root + '\Software\Microsoft\ColorFiltering'
$contrastKey = $root + '\Control Panel\Accessibility\HighContrast'

$active = Get-Number $filterKey 'Active'
$type = Get-Number $filterKey 'FilterType'
$hotkey = Get-Number $filterKey 'HotkeyEnabled'
$flags = Get-Number $contrastKey 'Flags'

$facts = [ordered]@{
    filter_on       = ($active -eq 1)
    filter_type     = ''
    filter_hotkey   = ''
    high_contrast   = ''
    contrast_hotkey = ''
}
if ($null -ne $type) {
    $facts.filter_type = $type
}
if ($null -ne $hotkey) {
    $facts.filter_hotkey = ($hotkey -eq 1)
}
if ($null -ne $flags) {
    $facts.high_contrast = (($flags -band 1) -ne 0)
    $facts.contrast_hotkey = (($flags -band 4) -ne 0)
}

$result = 'ok'
if ($facts.filter_on) {
    # FilterType missing means the default, grayscale.
    switch ($type) {
        1 { $result = 'inverted' }
        { ($_ -eq 3) -or ($_ -eq 4) -or ($_ -eq 5) } { $result = 'color-filter' }
        default { $result = 'grayscale' }
    }
}
elseif ($facts.high_contrast -eq $true) {
    $result = 'high-contrast'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
