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
# (ms-windows-store://pdp/?ProductId=9P7BP5VNWKX5) when the Microsoft Store
# (Microsoft.WindowsStore) is installed. Both go through the shell, which
# starts Store apps as the user, not elevated. Nothing is opened for a
# protocol whose app is not installed: Windows would show its "Pick an app"
# dialog instead.
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
$storePackage = 'Microsoft.WindowsStore'
$appUri = 'ms-quick-assist:'
$storeUri = 'ms-windows-store://pdp/?ProductId=9P7BP5VNWKX5'

# ---- shared block store-package: identical in tools/system/remote-help.ps1 and tools/system/file-recovery.ps1 (medkit-data check compares them) ----
# The logged-in user's SID from -UserHive (HKEY_USERS\<SID>); '' for HKCU:.
$userSid = ''
if ($UserHive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
    $userSid = $Matches[1]
}

# The Store package installed for the logged-in user, or $null (asking for
# another user needs administrator rights; without them the current user is
# asked).
function Get-UserPackage {
    param([string]$Name)
    $packages = @()
    try {
        if ($userSid.Length -gt 0) {
            $packages = @(Get-AppxPackage -User $userSid -Name $Name -ErrorAction Stop)
        }
        else {
            $packages = @(Get-AppxPackage -Name $Name -ErrorAction Stop)
        }
    }
    catch {
        Write-Verbose ('Get-AppxPackage failed: {0}' -f $_.Exception.Message)
        try {
            $packages = @(Get-AppxPackage -Name $Name -ErrorAction Stop)
        }
        catch {
            Write-Verbose ('Get-AppxPackage failed: {0}' -f $_.Exception.Message)
        }
    }
    if ($packages.Count -gt 0) {
        return $packages[0]
    }
    return $null
}
# ---- end of shared block store-package ----

$facts = [ordered]@{ error = '' }
$result = ''
if ($null -ne (Get-UserPackage $packageName)) {
    try {
        Start-Process -FilePath $appUri -ErrorAction Stop
        $result = 'opened'
    }
    catch {
        $result = 'failed'
        $facts.error = ([string]$_.Exception.Message).Trim()
    }
}
elseif ($null -ne (Get-UserPackage $storePackage)) {
    try {
        Start-Process -FilePath $storeUri -ErrorAction Stop
        $result = 'store'
    }
    catch {
        $result = 'no-store'
        $facts.error = ([string]$_.Exception.Message).Trim()
    }
}
else {
    $result = 'no-store'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
