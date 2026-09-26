# Tool: network.wifi-passwords
# Saved WLAN profiles with their passwords, so that a new phone can be connected
# to the same network. Read-only; needs administrator rights (netsh only writes
# the key in plain text for an administrator).
#
# 1. No WLAN AutoConfig service (wlansvc) installed or running, or no enabled
#    physical wireless adapter -> no-wifi (desktop PCs without wifi, servers,
#    the CI machine). Detected without throwing.
# 2. netsh wlan export profile key=clear folder=<folder> writes one XML file per
#    profile; the files are parsed as XML (never the localized console text of
#    netsh), and the exit code is checked.
# 3. The export puts plain-text passwords on disk, so the folder is created
#    under the temp directory with a random name, and BEFORE the export its
#    permissions are replaced by a protected ACL (no inheritance) that grants
#    full control to SYSTEM and BUILTIN\Administrators only; the ACL is read
#    back to make sure. The whole folder is deleted in "finally", also when
#    something failed; if it cannot be deleted the script throws so that the
#    user hears about it.
# 4. Passwords and XML text never go into facts or exception messages. Error
#    messages have the temp path (which contains the user name) replaced by
#    %TEMP%.
#
# Sections: one "wifi" section per profile (name = profile name, which is
# normally the SSID and may be Chinese). Rows:
#   password  the key as a secret value, or a code saying why there is none:
#             no-password (open network), account-login (enterprise network),
#             not-saved, unreadable (saved, but netsh returned it encrypted)
#   security  wpa3-personal / wpa2-personal / wpa-personal / wep / open / owe /
#             enterprise / other (MSM/security/authEncryption)
# The same profile on two wireless adapters is shown once.
#
# Result codes: found / none / no-wifi (all status ok). Facts: count, skipped
# (exported files that could not be read).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# SYSTEM and BUILTIN\Administrators.
$trustedSids = @('S-1-5-18', 'S-1-5-32-544')

$tempRoot = [System.IO.Path]::GetTempPath()

# Error messages without the temp path, which contains the user name.
function Protect-Message {
    param([string]$Message)
    $root = $tempRoot.TrimEnd([char[]]@('\', '/'))
    if ($root.Length -gt 3) {
        $Message = $Message.Replace($root, '%TEMP%')
    }
    return $Message
}

function Test-WlanService {
    $service = Get-Service -Name 'WlanSvc' -ErrorAction SilentlyContinue
    return (($null -ne $service) -and ([string]$service.Status -eq 'Running'))
}

# $true: an enabled physical wireless adapter exists. $false: none.
# $null: cannot tell (the NetAdapter cmdlets failed); netsh decides then.
function Test-WifiAdapter {
    try {
        $adapters = @(Get-NetAdapter -Physical)
    }
    catch {
        return $null
    }
    foreach ($a in $adapters) {
        # NdisPhysicalMedium 9 = native 802.11, 1 = wireless LAN; State 3 = disabled.
        $medium = [string]$a.NdisPhysicalMedium
        if ((($medium -eq '9') -or ($medium -eq '1')) -and ([string]$a.State -ne '3') -and ([string]$a.Status -ne 'Disabled')) {
            return $true
        }
    }
    return $false
}

# Creates the export folder and gives it a protected ACL for SYSTEM and
# Administrators only; throws unless the ACL reads back that way.
function New-ProtectedFolder {
    param([string]$Path)
    $null = [System.IO.Directory]::CreateDirectory($Path)
    $acl = New-Object System.Security.AccessControl.DirectorySecurity
    # Protected: nothing inherited from the temp folder, and no copies of it.
    $acl.SetAccessRuleProtection($true, $false)
    $inherit = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
    $propagate = [System.Security.AccessControl.PropagationFlags]::None
    $full = [System.Security.AccessControl.FileSystemRights]::FullControl
    $allow = [System.Security.AccessControl.AccessControlType]::Allow
    foreach ($sid in $trustedSids) {
        $identity = New-Object System.Security.Principal.SecurityIdentifier -ArgumentList $sid
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule -ArgumentList $identity, $full, $inherit, $propagate, $allow
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl

    $check = Get-Acl -LiteralPath $Path
    if (-not $check.AreAccessRulesProtected) {
        throw 'The temporary export folder still inherits permissions; nothing was exported'
    }
    foreach ($entry in @($check.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))) {
        if ($trustedSids -notcontains [string]$entry.IdentityReference.Value) {
            throw 'The temporary export folder is open to other accounts; nothing was exported'
        }
    }
}

# Deletes the export folder; retries for a while because a virus scanner may
# still be reading the new files. $true when the folder is gone.
function Remove-ExportFolder {
    param([string]$Path)
    for ($i = 0; $i -lt 10; $i++) {
        try {
            if (-not (Test-Path -LiteralPath $Path)) {
                return $true
            }
            Get-ChildItem -LiteralPath $Path -Force | Remove-Item -Force -Recurse
            Remove-Item -LiteralPath $Path -Force -Recurse
        }
        catch {
            Start-Sleep -Milliseconds 200
        }
    }
    return (-not (Test-Path -LiteralPath $Path))
}

function Read-XmlFile {
    param([string]$Path)
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [System.Xml.XmlReader]::Create($Path, $settings)
    try {
        $doc = New-Object System.Xml.XmlDocument
        # Keep the key material exactly as written (it may start or end with a space).
        $doc.PreserveWhitespace = $true
        $doc.Load($reader)
        # The unary comma keeps the pipeline from enumerating the document's nodes.
        return , $doc
    }
    finally {
        $reader.Close()
    }
}

# The element at a path of local names below $Node (the profile XML uses a
# default namespace), or $null.
function Get-XmlNode {
    param($Node, [string[]]$Names)
    $xpath = (@($Names | ForEach-Object { "*[local-name()='" + $_ + "']" })) -join '/'
    return $Node.SelectSingleNode($xpath)
}

function Get-XmlText {
    param($Node, [string[]]$Names)
    $found = Get-XmlNode -Node $Node -Names $Names
    if ($null -eq $found) {
        return ''
    }
    return ([string]$found.InnerText).Trim()
}

# SSID bytes as text when the profile has no readable name.
function ConvertFrom-SsidHex {
    param([string]$Hex)
    if (($Hex.Length -eq 0) -or (($Hex.Length % 2) -ne 0) -or ($Hex -notmatch '^[0-9A-Fa-f]+$')) {
        return $Hex
    }
    $bytes = New-Object byte[] ($Hex.Length / 2)
    for ($i = 0; $i -lt $bytes.Length; $i++) {
        $bytes[$i] = [System.Convert]::ToByte($Hex.Substring($i * 2, 2), 16)
    }
    try {
        # Strict UTF-8: invalid bytes throw instead of turning into '?'.
        $utf8 = New-Object System.Text.UTF8Encoding -ArgumentList $false, $true
        return $utf8.GetString($bytes)
    }
    catch {
        return $Hex
    }
}

function Get-SecurityCode {
    param([string]$Authentication, [string]$Encryption, [bool]$OneX)
    if ($OneX) {
        return 'enterprise'
    }
    switch ($Authentication) {
        'WPA3SAE' { return 'wpa3-personal' }
        'WPA2PSK' { return 'wpa2-personal' }
        'WPAPSK' { return 'wpa-personal' }
        'shared' { return 'wep' }
        'open' {
            if ($Encryption -eq 'WEP') {
                return 'wep'
            }
            return 'open'
        }
        'OWE' { return 'owe' }
        'WPA' { return 'enterprise' }
        'WPA2' { return 'enterprise' }
        'WPA3' { return 'enterprise' }
        'WPA3ENT' { return 'enterprise' }
        'WPA3ENT192' { return 'enterprise' }
    }
    return 'other'
}

# One exported profile: Name, Security, Password ('' when there is none) and
# Reason (the code shown instead of a password). $null when it has no name.
function Read-WlanProfile {
    param($Document)
    $root = $Document.DocumentElement
    if (($null -eq $root) -or ($root.LocalName -ne 'WLANProfile')) {
        return $null
    }
    $name = Get-XmlText -Node $root -Names @('name')
    if ($name.Length -eq 0) {
        $name = Get-XmlText -Node $root -Names @('SSIDConfig', 'SSID', 'name')
    }
    if ($name.Length -eq 0) {
        $name = ConvertFrom-SsidHex (Get-XmlText -Node $root -Names @('SSIDConfig', 'SSID', 'hex'))
    }
    if ($name.Length -eq 0) {
        return $null
    }

    $auth = Get-XmlText -Node $root -Names @('MSM', 'security', 'authEncryption', 'authentication')
    $encryption = Get-XmlText -Node $root -Names @('MSM', 'security', 'authEncryption', 'encryption')
    $oneX = (Get-XmlText -Node $root -Names @('MSM', 'security', 'authEncryption', 'useOneX')) -eq 'true'
    $security = Get-SecurityCode -Authentication $auth -Encryption $encryption -OneX $oneX

    $password = ''
    $reason = ''
    $keyNode = Get-XmlNode -Node $root -Names @('MSM', 'security', 'sharedKey', 'keyMaterial')
    $protected = Get-XmlText -Node $root -Names @('MSM', 'security', 'sharedKey', 'protected')
    if (($security -eq 'open') -or ($security -eq 'owe')) {
        $reason = 'no-password'
    }
    elseif ($security -eq 'enterprise') {
        $reason = 'account-login'
    }
    elseif (($null -eq $keyNode) -or ([string]$keyNode.InnerText).Length -eq 0) {
        $reason = 'not-saved'
    }
    elseif ($protected -ne 'false') {
        $reason = 'unreadable'
    }
    else {
        $password = [string]$keyNode.InnerText
    }

    return [pscustomobject]@{
        Name     = $name
        Security = $security
        Password = $password
        Reason   = $reason
    }
}

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$netsh = Join-Path $systemDir 'netsh.exe'

$result = 'no-wifi'
$profiles = New-Object System.Collections.Generic.List[object]
$skipped = 0

try {
    $adapter = $null
    $service = Test-WlanService
    if ($service) {
        $adapter = Test-WifiAdapter
    }
    if ($service -and ($false -ne $adapter)) {
        $folder = Join-Path $tempRoot ('medkit-wifi-' + [guid]::NewGuid().ToString('N'))
        try {
            New-ProtectedFolder -Path $folder
            if (-not (Test-Path -LiteralPath $netsh -PathType Leaf)) {
                throw 'netsh.exe was not found in System32'
            }
            # netsh prints a localized line per profile; it is discarded. With
            # ErrorActionPreference 'Stop', Windows PowerShell 5.1 would turn a
            # stderr line of a native command into a terminating error, so relax
            # it here and judge success by the exit code.
            $previousPreference = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            try {
                $null = & $netsh 'wlan' 'export' 'profile' ('folder=' + $folder) 'key=clear' 2>&1
                $exitCode = $LASTEXITCODE
            }
            finally {
                $ErrorActionPreference = $previousPreference
            }
            if ($exitCode -ne 0) {
                throw ('netsh wlan export profile exited with code {0}' -f $exitCode)
            }
            foreach ($file in @(Get-ChildItem -LiteralPath $folder -Filter '*.xml' -File)) {
                try {
                    $entry = Read-WlanProfile (Read-XmlFile $file.FullName)
                }
                catch {
                    # Never pass the parser message on: it could quote the file.
                    $entry = $null
                }
                if ($null -eq $entry) {
                    $skipped++
                }
                else {
                    $profiles.Add($entry)
                }
            }
        }
        finally {
            if (-not (Remove-ExportFolder -Path $folder)) {
                throw ('Could not delete the temporary folder %TEMP%\{0} with the exported profiles; please delete it by hand' -f (Split-Path -Leaf $folder))
            }
        }
        $result = 'none'
    }
}
catch {
    throw (Protect-Message $_.Exception.Message)
}

# The same network on a second wireless adapter is exported twice: keep one,
# preferably the copy with a readable password.
$unique = New-Object 'System.Collections.Generic.Dictionary[string,object]' -ArgumentList ([System.StringComparer]::Ordinal)
foreach ($p in $profiles) {
    if (-not $unique.ContainsKey($p.Name)) {
        $unique[$p.Name] = $p
    }
    elseif (($unique[$p.Name].Password.Length -eq 0) -and ($p.Password.Length -gt 0)) {
        $unique[$p.Name] = $p
    }
}

$sections = New-Object System.Collections.Generic.List[object]
foreach ($p in @($unique.Values | Sort-Object -Property Name)) {
    $rows = New-Object System.Collections.Generic.List[object]
    if ($p.Password.Length -gt 0) {
        $rows.Add([ordered]@{ id = 'password'; value = $p.Password; secret = $true })
    }
    else {
        $rows.Add([ordered]@{ id = 'password'; code = $p.Reason })
    }
    $rows.Add([ordered]@{ id = 'security'; code = $p.Security })
    $sections.Add([ordered]@{ id = 'wifi'; name = $p.Name; rows = $rows.ToArray() })
}
if ($sections.Count -gt 0) {
    $result = 'found'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{
        count   = $sections.Count
        skipped = $skipped
    }
    sections = $sections.ToArray()
}
