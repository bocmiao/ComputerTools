# Feature: explorer.jpeg-extension -- undo
# Puts the value back to what the run recorded (-Before = { where, value }):
# the old extension, or no value at all when there was none, in the same key.
# Nothing to do when neither key existed before the run.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:',
    [Parameter(Mandatory = $true)]
    [string]$Before
)

$ErrorActionPreference = 'Stop'

# ---- shared block jpeg-extension: identical in checks/system/jpeg-extension.ps1 and features/explorer/jpeg-extension-detect.ps1, jpeg-extension-run.ps1, jpeg-extension-undo.ps1 and jpeg-extension-break.ps1 (medkit-data check compares them) ----
# Chrome, Edge and other programs ask Windows which file name extension goes
# with the MIME type image/jpeg: the value Extension of HKEY_CLASSES_ROOT\MIME\
# Database\Content Type\image/jpeg (Chromium's
# GetPlatformPreferredExtensionForMimeType reads exactly that). When it is
# .jfif, pictures saved from web pages end in .jfif.
# HKEY_CLASSES_ROOT merges the user's Software\Classes with the machine's, and
# Mime\Database\Content Type is one of the keys Microsoft lists in "Merged
# View of HKEY_CLASSES_ROOT": when the user has a subkey of it (image/jpeg),
# nothing of the machine's subkey of the same name shows through. So the value
# that counts is the user's when the user has an image/jpeg key (even when that
# key has no Extension), else the machine's. Only that value is read and
# changed; no key is ever created (a new user key would hide the machine's
# other values, such as CLSID).
# The key name has a slash, which the PowerShell registry provider takes for a
# path separator, so .NET registry keys are used (64-bit view).
$jpegKeyPath = 'Software\Classes\MIME\Database\Content Type\image/jpeg'
$jpegValueName = 'Extension'
# The logged-in user's SID from -UserHive ('Registry::HKEY_USERS\<SID>'), or ''
# when it is this process's HKEY_CURRENT_USER.
$jpegUserSid = ''
if ($UserHive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
    $jpegUserSid = $Matches[1]
}

# The image/jpeg key of the logged-in user ('user': HKEY_USERS\<SID>, else
# this process's HKEY_CURRENT_USER) or of the machine ('machine'), or $null
# when that key does not exist.
function Open-JpegKey {
    param([string]$Where, [bool]$Writable)
    $view = [Microsoft.Win32.RegistryView]::Registry64
    $path = $jpegKeyPath
    if ($Where -eq 'machine') {
        $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $view)
    }
    elseif ($jpegUserSid.Length -gt 0) {
        $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::Users, $view)
        $path = $jpegUserSid + '\' + $jpegKeyPath
    }
    else {
        $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser, $view)
    }
    try {
        return $base.OpenSubKey($path, $Writable)
    }
    finally {
        $base.Close()
    }
}

# Where the value that counts is ('user', 'machine', or '' when neither has an
# image/jpeg key) and what it is (Value: $null when missing or not text).
function Get-JpegExtension {
    foreach ($where in @('user', 'machine')) {
        $key = Open-JpegKey -Where $where -Writable $false
        if ($null -eq $key) {
            continue
        }
        try {
            $value = $key.GetValue($jpegValueName, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        }
        finally {
            $key.Close()
        }
        if ($value -isnot [string]) {
            $value = $null
        }
        return [pscustomobject]@{ Where = $where; Value = $value }
    }
    return [pscustomobject]@{ Where = ''; Value = $null }
}

# True for the extensions pictures should be saved with: .jpg or .jpeg.
function Test-JpegGood {
    param($Value)
    return (($Value -is [string]) -and (@('.jpg', '.jpeg') -contains $Value.Trim()))
}

# Sets the value in the image/jpeg key of $Where to $Value (text), or removes
# it when $Value is $null. Throws when that key is gone.
function Set-JpegExtension {
    param([string]$Where, $Value)
    $key = Open-JpegKey -Where $Where -Writable $true
    if ($null -eq $key) {
        throw ('the image/jpeg key of the ' + $Where + ' is missing')
    }
    try {
        if ($null -eq $Value) {
            $key.DeleteValue($jpegValueName, $false)
        }
        else {
            $key.SetValue($jpegValueName, [string]$Value, [Microsoft.Win32.RegistryValueKind]::String)
        }
    }
    finally {
        $key.Close()
    }
}
# ---- end of shared block jpeg-extension ----

$recorded = ConvertFrom-Json -InputObject $Before
$where = [string]$recorded.where
if ($where.Length -eq 0) {
    return [pscustomobject]@{ restored = [ordered]@{ where = ''; value = $null } }
}
$value = $null
if ($null -ne $recorded.value) {
    $value = [string]$recorded.value
}
Set-JpegExtension -Where $where -Value $value

[pscustomobject]@{ restored = [ordered]@{ where = $where; value = $value } }
