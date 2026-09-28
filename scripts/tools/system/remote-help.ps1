# Tool: system.remote-help (action)
# Opens Quick Assist ("kuai su zhu shou"), the remote help that comes with
# Windows: someone the user knows sees the screen, and controls it when the
# user allows, to help. The confirmation in the YAML carries the warning
# about scams (Microsoft, "Use Quick Assist to help users": only let a helper
# connect when you started the contact yourself).
# Quick Assist is a Microsoft Store app (package
# MicrosoftCorporationII.QuickAssist, product 9P7BP5VNWKX5). When it is
# installed for the logged-in user (the SID in -UserHive; the current user
# when that cannot be read) it is started through its protocol
# ms-quick-assist:, otherwise its Store page is opened
# (ms-windows-store://pdp/?ProductId=9P7BP5VNWKX5). Both go through the
# shell, which starts Store apps as the user, not elevated.
# Result codes: opened / store (the Store page was opened to install it) /
# no-store (neither could be opened: no Microsoft Store, as on Windows
# Server or a stripped-down Windows) / failed (installed, but it did not
# start). Facts: error (the error text, for failed and no-store).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$packageName = 'MicrosoftCorporationII.QuickAssist'
$appUri = 'ms-quick-assist:'
$storeUri = 'ms-windows-store://pdp/?ProductId=9P7BP5VNWKX5'

$userSid = ''
if ($UserHive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
    $userSid = $Matches[1]
}

# Quick Assist is installed for the logged-in user (asking for another user
# needs administrator rights; without them the current user is asked).
function Test-QuickAssist {
    $packages = @()
    try {
        if ($userSid.Length -gt 0) {
            $packages = @(Get-AppxPackage -User $userSid -Name $packageName -ErrorAction Stop)
        }
        else {
            $packages = @(Get-AppxPackage -Name $packageName -ErrorAction Stop)
        }
    }
    catch {
        Write-Verbose ('Get-AppxPackage failed: {0}' -f $_.Exception.Message)
        try {
            $packages = @(Get-AppxPackage -Name $packageName -ErrorAction Stop)
        }
        catch {
            Write-Verbose ('Get-AppxPackage failed: {0}' -f $_.Exception.Message)
        }
    }
    return ($packages.Count -gt 0)
}

$facts = [ordered]@{ error = '' }
$result = ''
if (Test-QuickAssist) {
    try {
        Start-Process -FilePath $appUri -ErrorAction Stop
        $result = 'opened'
    }
    catch {
        $result = 'failed'
        $facts.error = ([string]$_.Exception.Message).Trim()
    }
}
else {
    try {
        Start-Process -FilePath $storeUri -ErrorAction Stop
        $result = 'store'
    }
    catch {
        $result = 'no-store'
        $facts.error = ([string]$_.Exception.Message).Trim()
    }
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
