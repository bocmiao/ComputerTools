# Check: security.uac
# Is User Account Control on? HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\
# Policies\System (Microsoft, "User Account Control settings and
# configuration"):
#   EnableLUA                   0 = UAC is off altogether: every program runs
#                               with full administrator rights, and apps
#                               from the Store (Photos, Calculator) may not
#                               start. 1 or missing = on (the default).
#   ConsentPromptBehaviorAdmin  0 = "Never notify" (elevate without asking);
#                               5 or missing = the default.
#   PromptOnSecureDesktop       0 = the prompt does not dim the desktop (a
#                               level of the slider, a choice of the user:
#                               not reported).
# "Optimizing" guides and tools often turn UAC off. Medkit never writes these
# values (plan, section 5): the user moves the slider back in the UAC
# settings window.
# Result codes: ok / never-notify / off.
# Facts: enable_lua, consent_admin, secure_desktop (the values, -1 when
# missing).
# Read-only.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$key = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'

function Get-Number {
    param($Item, [string]$Name)
    if ($null -eq $Item) {
        return -1
    }
    $property = $Item.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return -1
    }
    $number = 0
    if ([int]::TryParse([string]$property.Value, [ref]$number)) {
        return $number
    }
    return -1
}

$item = $null
try {
    $item = Get-ItemProperty -LiteralPath $key -ErrorAction Stop
}
catch {
    $item = $null
}
$enableLua = Get-Number $item 'EnableLUA'
$consentAdmin = Get-Number $item 'ConsentPromptBehaviorAdmin'
$secureDesktop = Get-Number $item 'PromptOnSecureDesktop'

$result = 'ok'
if ($enableLua -eq 0) {
    $result = 'off'
}
elseif ($consentAdmin -eq 0) {
    $result = 'never-notify'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        enable_lua     = $enableLua
        consent_admin  = $consentAdmin
        secure_desktop = $secureDesktop
    }
}
