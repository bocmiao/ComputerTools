# Check: system.managed-device
# Is this PC managed by an organization?
#   1. Joined to an Active Directory domain: Win32_ComputerSystem.PartOfDomain
#   2. Joined to Microsoft Entra ID (Azure AD): subkeys under
#      HKLM\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo
#   3. Enrolled in MDM (Intune or another MDM): a subkey of
#      HKLM\SOFTWARE\Microsoft\Enrollments that looks like a real enrollment
# Read-only. Result codes: personal / managed.
#
# Microsoft does not document the Enrollments key. Windows 10/11 create about
# thirty bookkeeping subkeys there even on home PCs, and several of them carry a
# ProviderID ("Local Authority", "Cloud Authority", "Deploy Authority",
# "WMI_Bridge_SCCM_Server", ...). So an enrollment only counts when:
#   - ProviderID is set and is not one of the internal pseudo providers below,
#   - DiscoveryServiceFullURL is set (the MDM service it enrolled with), and
#   - EnrollmentState is 1 (enrolled) when the value is present; failed or
#     removed enrollments can leave their key behind with another state.
# Intune uses ProviderID "MS DM Server"; Windows declared configuration (MMP-C)
# uses "Microsoft Device Management"; other MDMs use their own names.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Test-RegistryKey {
    param([string]$Path)
    try {
        $null = Get-Item -LiteralPath $Path -ErrorAction Stop
        return $true
    }
    catch [System.Management.Automation.ItemNotFoundException] {
        return $false
    }
}

function Get-PropertyText {
    param($Object, [string]$Name)
    if ($null -eq $Object) {
        return ''
    }
    $prop = $Object.PSObject.Properties[$Name]
    if (($null -eq $prop) -or ($null -eq $prop.Value)) {
        return ''
    }
    return ([string]$prop.Value).Trim()
}

function Test-AccessDenied {
    param($ErrorRecord)
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        if (($ex -is [System.Security.SecurityException]) -or ($ex -is [System.UnauthorizedAccessException])) {
            return $true
        }
        $ex = $ex.InnerException
    }
    return $false
}

$internalProviders = @(
    'Local Authority', 'Cloud Authority', 'Deploy Authority', 'SC Authority', 'MS Work Account',
    'WMI_Bridge_Server', 'WMI_Bridge_SCCM_Server', 'Local_Management'
)

$computer = Get-CimInstance -ClassName Win32_ComputerSystem
$domainJoined = [bool]$computer.PartOfDomain

$joinInfoPath = 'HKLM:\SYSTEM\CurrentControlSet\Control\CloudDomainJoin\JoinInfo'
$entraJoined = $false
if (Test-RegistryKey $joinInfoPath) {
    $entraJoined = ((Get-Item -LiteralPath $joinInfoPath).SubKeyCount -gt 0)
}

$providers = New-Object System.Collections.Generic.List[string]
$unreadable = 0
$enrollmentsPath = 'HKLM:\SOFTWARE\Microsoft\Enrollments'
if (Test-RegistryKey $enrollmentsPath) {
    foreach ($name in @((Get-Item -LiteralPath $enrollmentsPath).GetSubKeyNames())) {
        $values = $null
        try {
            $values = Get-ItemProperty -LiteralPath (Join-Path $enrollmentsPath $name) -ErrorAction Stop
        }
        catch {
            if (Test-AccessDenied $_) {
                $unreadable++
                continue
            }
            throw
        }
        $provider = Get-PropertyText $values 'ProviderID'
        if ($provider.Length -eq 0) {
            continue
        }
        if ($internalProviders -contains $provider) {
            continue
        }
        if ((Get-PropertyText $values 'DiscoveryServiceFullURL').Length -eq 0) {
            continue
        }
        $state = Get-PropertyText $values 'EnrollmentState'
        if (($state.Length -gt 0) -and ($state -ne '1')) {
            continue
        }
        if (-not $providers.Contains($provider)) {
            $providers.Add($provider)
        }
    }
}

$mdmEnrolled = ($providers.Count -gt 0)
$managed = $domainJoined -or $entraJoined -or $mdmEnrolled

# An unreadable key might be the enrollment; do not guess "personal".
if ((-not $managed) -and ($unreadable -gt 0)) {
    throw ('{0} enrollment key(s) could not be read; cannot rule out MDM enrollment' -f $unreadable)
}

$facts = [ordered]@{
    domain_joined   = $domainJoined
    azure_ad_joined = $entraJoined
    mdm_enrolled    = $mdmEnrolled
}
if ($mdmEnrolled) {
    $facts['mdm_provider'] = ($providers -join ', ')
}

$result = 'personal'
if ($managed) {
    $result = 'managed'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
