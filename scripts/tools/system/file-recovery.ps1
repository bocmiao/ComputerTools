# Tool: system.file-recovery (action)
# Opens Microsoft's Windows File Recovery (winfr), the free Microsoft Store
# app that recovers deleted files from the command line (package
# Microsoft.WindowsFileRecovery, product 9N26S50LN705; Microsoft, "Windows
# File Recovery"). The medkit page builds the winfr command for the user;
# this tool only opens the app, or its Store page when it is not installed.
# Same pattern as system.remote-help: the package is looked up for the
# logged-in user (the SID in -UserHive; the current user when that cannot be
# read) and the app is started through the shell by its AppUserModelID
# (shell:AppsFolder\<package family name>!<application id from its
# manifest>). The app asks for administrator rights itself (Microsoft: when
# prompted, select Yes) and shows its console window. Nothing is opened for
# an app or a Store that is not installed.
# Result codes: opened / installed (installed, but it could not be opened
# from here) / store (its Store page was opened to install it) / no-store
# (not installed and there is no Microsoft Store, as on Windows Server or a
# stripped-down Windows). Facts: error (the error text, for installed and
# no-store).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$packageName = 'Microsoft.WindowsFileRecovery'
$storePackage = 'Microsoft.WindowsStore'
$storeUri = 'ms-windows-store://pdp/?ProductId=9N26S50LN705'

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

# The first application id in the package manifest ('' when it cannot be read).
function Get-ApplicationId {
    param($Package)
    try {
        if ($userSid.Length -gt 0) {
            $manifest = Get-AppxPackageManifest -Package $Package.PackageFullName -User $userSid -ErrorAction Stop
        }
        else {
            $manifest = Get-AppxPackageManifest -Package $Package.PackageFullName -ErrorAction Stop
        }
        $apps = @($manifest.Package.Applications.Application)
        if ($apps.Count -gt 0 -and $apps[0].Id) {
            return [string]$apps[0].Id
        }
    }
    catch {
        Write-Verbose ('Get-AppxPackageManifest failed: {0}' -f $_.Exception.Message)
    }
    return ''
}

$facts = [ordered]@{ error = '' }
$result = ''
$package = Get-UserPackage $packageName
if ($null -ne $package) {
    $appId = Get-ApplicationId $package
    if ($appId.Length -gt 0) {
        try {
            Start-Process -FilePath ('shell:AppsFolder\{0}!{1}' -f $package.PackageFamilyName, $appId) -ErrorAction Stop
            $result = 'opened'
        }
        catch {
            $result = 'installed'
            $facts.error = ([string]$_.Exception.Message).Trim()
        }
    }
    else {
        $result = 'installed'
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
