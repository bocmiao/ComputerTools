# Check: security.bitlocker-status
# Are any fixed disks encrypted with BitLocker (including the automatic "device
# encryption" of Windows 11 24H2)? If so, the user must know where the recovery
# key is before reinstalling Windows, replacing the motherboard or updating the BIOS.
# Source: Win32_EncryptableVolume in root\cimv2\Security\MicrosoftVolumeEncryption.
#   VolumeType:       0 = OS volume, 1 = fixed data volume, 2 = removable (skipped)
#   ProtectionStatus: 0 = off, 1 = on, 2 = unknown (for example a locked volume)
#   ConversionStatus: 0 = fully decrypted, 1 = fully encrypted, 2..5 = in progress / paused
# A volume counts as encrypted when protection is on or unknown, or when any part
# of it is encrypted (this also covers "suspended" protection and device
# encryption that is still waiting for activation).
# Read-only. Needs administrator rights. Result codes: off / on. Errors (for
# example a missing WMI namespace) are thrown and shown as "unknown".

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$volumes = @(Get-CimInstance -Namespace 'root\cimv2\Security\MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume)

$encrypted = New-Object System.Collections.Generic.List[string]
$protectionOn = $false
$converting = $false
$fixedCount = 0

foreach ($v in $volumes) {
    if (($null -ne $v.VolumeType) -and ([int]$v.VolumeType -eq 2)) {
        continue
    }
    # Volumes without a drive letter (rare on fixed disks) cannot be named to the
    # user and are left out.
    $letter = ([string]$v.DriveLetter).Trim().ToUpperInvariant()
    if ($letter.Length -eq 0) {
        continue
    }
    $fixedCount++

    $protection = [int]$v.ProtectionStatus
    $conversion = [int]$v.ConversionStatus
    if ($protection -eq 1) {
        $protectionOn = $true
    }
    if (($conversion -ge 2) -and ($conversion -le 5)) {
        $converting = $true
    }
    if (($protection -ne 0) -or ($conversion -ne 0)) {
        $encrypted.Add($letter)
    }
}

$encrypted.Sort()

$result = 'off'
if ($encrypted.Count -gt 0) {
    $result = 'on'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        encrypted_volumes = ($encrypted -join ', ')
        encrypted_count   = $encrypted.Count
        volume_count      = $fixedCount
        protection_on     = $protectionOn
        converting        = $converting
    }
}
