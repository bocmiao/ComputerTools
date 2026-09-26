# Check: security.tpm
# TPM (Trusted Platform Module) presence and version, for Windows 11 readiness.
# Source: Win32_Tpm in root\cimv2\Security\MicrosoftTpm.
#   no instance                         -> missing (usually switched off in the BIOS:
#                                          Intel PTT / AMD fTPM)
#   SpecVersion "1.2, ..."              -> tpm12 (Windows 11 needs 2.0)
#   IsEnabled / IsActivated false       -> disabled
#   SpecVersion "2.0, ..."              -> tpm2
# SpecVersion looks like "2.0, 0, 1.38" (major.minor, revision level, errata).
# Read-only. Needs administrator rights.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$tpms = @(Get-CimInstance -Namespace 'root\cimv2\Security\MicrosoftTpm' -ClassName Win32_Tpm)

if ($tpms.Count -eq 0) {
    [pscustomobject]@{
        result = 'missing'
        facts  = [ordered]@{ present = $false }
    }
    return
}

$tpm = $tpms[0]

$specText = ([string]$tpm.SpecVersion).Trim()
$spec = ''
if (($specText.Length -gt 0) -and ($specText -ne 'Not Supported')) {
    $spec = $specText.Split(',')[0].Trim()
}

# A missing (null) value is treated as "not known to be off".
$enabled = $true
if ($null -ne $tpm.IsEnabled_InitialValue) {
    $enabled = [bool]$tpm.IsEnabled_InitialValue
}
$activated = $true
if ($null -ne $tpm.IsActivated_InitialValue) {
    $activated = [bool]$tpm.IsActivated_InitialValue
}

# ManufacturerId packs a short ASCII vendor code into 4 bytes, e.g. "INTC", "AMD", "IFX".
$manufacturer = ''
if ($null -ne $tpm.ManufacturerId) {
    $idValue = [uint32]$tpm.ManufacturerId
    if ($idValue -ne 0) {
        $chars = New-Object System.Collections.Generic.List[char]
        foreach ($shift in @(24, 16, 8, 0)) {
            $b = [int](($idValue -shr $shift) -band 0xFF)
            if (($b -ge 0x20) -and ($b -le 0x7E)) {
                $chars.Add([char]$b)
            }
        }
        $manufacturer = (-join $chars.ToArray()).Trim()
    }
}

$facts = [ordered]@{
    present      = $true
    spec_version = $spec
    enabled      = $enabled
    activated    = $activated
}
if ($manufacturer.Length -gt 0) {
    $facts['manufacturer'] = $manufacturer
}

# TPM 1.2 is reported as such even when switched off: turning it on would not
# make the PC ready for Windows 11.
if ($spec -like '1.*') {
    $result = 'tpm12'
}
elseif ((-not $enabled) -or (-not $activated)) {
    $result = 'disabled'
}
elseif ($spec -like '2.*') {
    $result = 'tpm2'
}
else {
    throw ("Unrecognized TPM SpecVersion '{0}'" -f $specText)
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
