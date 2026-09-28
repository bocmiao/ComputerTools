# Feature: system.utf8-beta-off -- undo
# Writes back the code pages recorded before the run (-Before; '' removes the
# value). Used after a restart.

[CmdletBinding()]
param(
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block code-page: identical in checks/system/code-page.ps1, checks/system/system-locale.ps1 and features/system/utf8-beta-*.ps1 (medkit-data check compares them) ----
# The system locale ("Language for non-Unicode programs") is the default value
# of HKLM\SYSTEM\CurrentControlSet\Control\Nls\Locale, an LCID in hex
# ("00000804"). The code pages non-Unicode programs use are ACP (ANSI), OEMCP
# (console) and MACCP (Mac) under Nls\CodePage, REG_SZ. For a Chinese system
# locale Windows uses 936 / 936 / 10008 (Simplified: zh-CN 0x0804, zh-SG
# 0x1004) or 950 / 950 / 10002 (Traditional: zh-TW 0x0404, zh-HK 0x0C04,
# zh-MO 0x1404). "Beta: Use Unicode UTF-8 for worldwide language support" in
# the Region settings sets all three to 65001; clearing it puts the defaults
# back.
$nlsKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Nls'
$codePageNames = @('ACP', 'OEMCP', 'MACCP')

# A registry value as trimmed text, '' when the key or the value is missing.
function Get-NlsText {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -ErrorAction Stop
    }
    catch {
        return ''
    }
    $prop = $item.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return ''
    }
    return ([string]$prop.Value).Trim()
}

# The system locale as an LCID number, or -1 when it cannot be read.
function Get-SystemLocaleId {
    $text = Get-NlsText ($nlsKey + '\Locale') '(default)'
    $n = 0
    if (($text.Length -gt 0) -and [int]::TryParse($text, [System.Globalization.NumberStyles]::HexNumber, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$n)) {
        return $n
    }
    return -1
}

# The code pages now, as an ordered table ACP / OEMCP / MACCP ('' when missing).
function Get-CodePages {
    $pages = [ordered]@{}
    foreach ($name in $codePageNames) {
        $pages[$name] = Get-NlsText ($nlsKey + '\CodePage') $name
    }
    return $pages
}

# The code pages Windows uses for a Chinese system locale without UTF-8, or
# $null for any other locale.
function Get-ChineseCodePages {
    param([int]$Lcid)
    if (@(0x0804, 0x1004) -contains $Lcid) {
        return [ordered]@{ ACP = '936'; OEMCP = '936'; MACCP = '10008' }
    }
    if (@(0x0404, 0x0C04, 0x1404) -contains $Lcid) {
        return [ordered]@{ ACP = '950'; OEMCP = '950'; MACCP = '10002' }
    }
    return $null
}

# True when every code page now is the expected one.
function Test-CodePages {
    param($Now, $Expected)
    foreach ($name in $codePageNames) {
        if ([string]$Now[$name] -ne [string]$Expected[$name]) {
            return $false
        }
    }
    return $true
}
# ---- end of shared block code-page ----

$codePageKey = $nlsKey + '\CodePage'
$recorded = ConvertFrom-Json -InputObject $Before
$map = @{ ACP = 'acp'; OEMCP = 'oemcp'; MACCP = 'maccp' }
foreach ($name in $codePageNames) {
    $value = [string]$recorded.($map[$name])
    if ($value.Length -eq 0) {
        Remove-ItemProperty -LiteralPath $codePageKey -Name $name -ErrorAction SilentlyContinue
    }
    else {
        Set-ItemProperty -LiteralPath $codePageKey -Name $name -Value $value -Type String -ErrorAction Stop
    }
}

[pscustomobject]@{ result = 'ok' }
