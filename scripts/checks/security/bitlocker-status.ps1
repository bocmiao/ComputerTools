# Check: security.bitlocker-status
# Are any fixed disks encrypted with BitLocker (including the automatic "device
# encryption" of Windows 11 24H2)? If so, the user must know where the recovery
# key is before reinstalling Windows, replacing the motherboard or updating the BIOS.
# Source: Win32_EncryptableVolume in root\cimv2\Security\MicrosoftVolumeEncryption.
#   VolumeType:       0 = OS volume, 1 = fixed data volume, 2 = removable (skipped)
#   ProtectionStatus: 0 = off, 1 = on, 2 = unknown (for example a locked volume)
#   ConversionStatus: 0 = fully decrypted, 1 = fully encrypted, 2..5 = in progress / paused
# A volume counts as encrypted when protection is on or unknown, or when any part
# of it is encrypted.
# Encrypted with protection off is either "suspended" (key protectors exist, a
# recovery key will be needed later) or device encryption that is still waiting
# for activation: encrypted with a clear key and no key protector at all, which
# Microsoft describes as "unprotected even though the data is encrypted". The two
# are told apart with GetKeyProtectors; when that call fails the volume counts as
# encrypted, the safer assumption.
# Read-only. Needs administrator rights. Result codes:
#   off      nothing encrypted
#   on       at least one volume is protected, suspended or locked
#   waiting  only volumes that are encrypted but waiting for activation
# Errors (for example a missing WMI namespace) are thrown and shown as "unknown".

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# Number of key protectors of the volume, or -1 when it cannot be read.
function Get-KeyProtectorCount {
    param($Volume)
    try {
        $r = Invoke-CimMethod -InputObject $Volume -MethodName GetKeyProtectors -Arguments @{ KeyProtectorType = [uint32]0 } -ErrorAction Stop
        if (($null -eq $r) -or ([int]$r.ReturnValue -ne 0)) {
            return -1
        }
        return @($r.VolumeKeyProtectorID | Where-Object { -not [string]::IsNullOrEmpty([string]$_) }).Count
    }
    catch {
        return -1
    }
}

$volumes = @(Get-CimInstance -Namespace 'root\cimv2\Security\MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume)

$encrypted = New-Object System.Collections.Generic.List[string]
$waiting = New-Object System.Collections.Generic.List[string]
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
    if (($protection -eq 0) -and ($conversion -eq 0)) {
        continue
    }
    if (($protection -eq 0) -and ((Get-KeyProtectorCount $v) -eq 0)) {
        $waiting.Add($letter)
    }
    else {
        $encrypted.Add($letter)
    }
}

$encrypted.Sort()
$waiting.Sort()

$result = 'off'
if ($encrypted.Count -gt 0) {
    $result = 'on'
}
elseif ($waiting.Count -gt 0) {
    $result = 'waiting'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        encrypted_volumes = ($encrypted -join ', ')
        waiting_volumes   = ($waiting -join ', ')
        encrypted_count   = $encrypted.Count
        volume_count      = $fixedCount
        protection_on     = $protectionOn
        converting        = $converting
    }
}
