# Check: privacy.camera
# Whether the privacy switches of Windows let programs use the camera.
# Read-only. Outputs one object: { result, facts }.
# Result codes: ok / policy-deny / system-off / desktop-off / apps-off (see the
# shared block below; texts live in catalog/checks/privacy/). Facts: policy
# (0, 1 or 2), system, desktop, apps ("Allow", "Deny" or "" when not set).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

# ---- shared block consent-store: identical in checks/privacy/microphone.ps1 and checks/privacy/camera.ps1 (medkit-data check compares them) ----
# The switches of Settings > Privacy (CapabilityAccessManager\ConsentStore\<capability>, string
# value "Value", "Allow" or "Deny"; missing means Allow), checked in the order that blocks most:
#   policy   HKLM\SOFTWARE\Policies\Microsoft\Windows\AppPrivacy, DWORD <policy name>:
#            2 = force deny (0 = the user decides, 1 = force allow)
#   system   HKLM ConsentStore\<capability>: the switch for the whole PC (all accounts)
#   desktop  <user hive> ConsentStore\<capability>\NonPackaged: "Let desktop apps access ...",
#            the switch for downloaded programs (meeting apps, WeChat, QQ)
#   apps     <user hive> ConsentStore\<capability>: "Let apps access ...", Store apps
# Only these switches are read: not the per-app entries and not their usage history.
$storePath = 'SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\'

function Get-ConsentValue {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return ''
    }
    $item = Get-ItemProperty -LiteralPath $Path
    if ($null -eq $item) {
        return ''
    }
    $property = $item.PSObject.Properties['Value']
    if (($null -eq $property) -or ($null -eq $property.Value)) {
        return ''
    }
    return ([string]$property.Value).Trim()
}

function Get-ConsentState {
    param([string]$UserHive, [string]$Capability, [string]$PolicyName)
    $policy = 0
    $policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\AppPrivacy'
    if (Test-Path -LiteralPath $policyKey) {
        # A key without values (a policy set and cleared again) gives nothing.
        $values = Get-ItemProperty -LiteralPath $policyKey
        $property = $null
        if ($null -ne $values) {
            $property = $values.PSObject.Properties[$PolicyName]
        }
        if ($null -ne $property) {
            $policy = [int]$property.Value
        }
    }
    $userRoot = $UserHive.TrimEnd('\')
    $state = [ordered]@{
        policy  = $policy
        system  = Get-ConsentValue ('HKLM:\' + $storePath + $Capability)
        desktop = Get-ConsentValue ($userRoot + '\' + $storePath + $Capability + '\NonPackaged')
        apps    = Get-ConsentValue ($userRoot + '\' + $storePath + $Capability)
    }
    $result = 'ok'
    if ($policy -eq 2) {
        $result = 'policy-deny'
    }
    elseif ($state.system -eq 'Deny') {
        $result = 'system-off'
    }
    elseif ($state.desktop -eq 'Deny') {
        $result = 'desktop-off'
    }
    elseif ($state.apps -eq 'Deny') {
        $result = 'apps-off'
    }
    return [pscustomobject]@{ result = $result; facts = $state }
}
# ---- end of shared block consent-store ----

Get-ConsentState -UserHive $UserHive -Capability 'webcam' -PolicyName 'LetAppsAccessCamera'
