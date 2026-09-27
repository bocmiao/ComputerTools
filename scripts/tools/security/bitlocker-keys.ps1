# Tool: security.bitlocker-keys
# The BitLocker recovery keys (48-digit recovery passwords) of the fixed
# volumes, so that the user can write them down BEFORE reinstalling Windows,
# replacing the motherboard or updating the BIOS (docs/plan.md 4.6, step 1).
# Read-only: no key protector is added, removed or backed up, no setting is
# changed, nothing is written to disk, nothing goes over the network.
#
# Source: Win32_EncryptableVolume in root\cimv2\Security\MicrosoftVolumeEncryption.
# Never the localized text of manage-bde; Get-BitLockerVolume is not used
# because it may be missing on Home editions. Same rules as
# checks/security/bitlocker-status.ps1:
#   VolumeType:       0 = OS volume, 1 = fixed data volume, 2 = removable (skipped)
#   ProtectionStatus: 0 = off, 1 = on, 2 = unknown (for example a locked volume)
#   ConversionStatus: 0 = fully decrypted, 1 = fully encrypted,
#                     2 / 3 = encryption / decryption in progress, 4 / 5 = paused
# Live values come from GetProtectionStatus and GetConversionStatus; the
# instance properties are used when a call fails. A volume counts as BitLocker
# unless protection is off and nothing is encrypted. Protection off and no key
# protector at all (GetKeyProtectors, type 0, returns nothing or
# FVE_E_NOT_ACTIVATED) is device encryption waiting for activation: there is
# no recovery key yet, and none is needed.
# Unlike the check, volumes without a drive letter are listed as well.
#
# Sections: one per BitLocker volume, OS volume first, then by drive letter.
# Id os_volume or data_volume with the drive letter as name, or no_letter.
# Rows (ids and codes are labelled in the YAML):
#   status            "on" / suspended / locked / encrypting / encryption-paused /
#                     decrypting / decryption-paused / waiting / unknown
#   encrypt_progress  "42%" while encrypting or paused (EncryptionPercentage)
#   decrypt_progress  "58%" while decrypting or paused (100 - EncryptionPercentage)
#   per numerical password protector (GetKeyProtectors, type 3):
#     key_id          the first 8 characters of the protector ID; the recovery
#                     screen shows them to tell the keys apart (not a secret)
#     recovery_key    the 48-digit password as a SECRET value
#                     (GetKeyProtectorNumericalPassword), or the code locked-key
#                     (FVE_E_LOCKED_VOLUME) or unreadable
#     backup          backed-up, only when event 845 of the BitLocker management
#                     log names this protector (see below)
#   recovery_key no-key: the volume has no numerical password (TPM only, ...);
#   recovery_key not-needed: the volume is waiting for activation;
#   recovery_key unreadable (locked-key on a locked volume): the protectors
#   could not be listed.
#
# Backup hint: Microsoft's Intune BitLocker troubleshooting article shows event
# 845 of Microsoft-Windows-BitLocker-API as a normal event ("BitLocker Drive
# Encryption recovery information for volume C: was backed up successfully to
# your ...", with "Protector GUID"); 846 is the failed backup to Microsoft
# Entra ID. The GUIDs in the event data are compared with the protector IDs;
# the localized message text is never read. No event is NOT proof of a missing
# backup (Microsoft does not document such an event for Microsoft account
# backups, and the log wraps), so nothing is said then. Reading the log is
# best effort: any failure just means no hints.
#
# Secrets: a recovery password only ever goes into a row with secret = true.
# It never goes into facts or error messages, and it is never handed to a
# cmdlet (module logging records parameter values): only -replace, -join and
# .NET string / regex methods touch it, and it lives in local variables and its
# row only.
# Failures while reading a password are turned into a code without keeping the
# message; any unexpected error after the volume list was read is rethrown
# without its original message, which could quote a value.
#
# Result codes: none / unsupported / waiting / found (ok), no-recovery-key and
# locked (advice), partial (unknown: some keys could not be read).
# unsupported: the WMI namespace or class does not exist, or WMI says "not
# supported" (Windows Server without the BitLocker feature, stripped-down
# systems); no error. Other failures to list the volumes are thrown.
# Facts (numbers only): volume_count, waiting_count, key_count, missing_count,
# locked_count, unreadable_count, backed_up_count.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$invariant = [System.Globalization.CultureInfo]::InvariantCulture
$namespace = 'root\cimv2\Security\MicrosoftVolumeEncryption'
$managementLog = 'Microsoft-Windows-BitLocker/BitLocker Management'
$backupEventId = 845

# FVE_E_LOCKED_VOLUME (0x80310000) and FVE_E_NOT_ACTIVATED (0x80310008).
$lockedVolumeCode = [long]2150694912
$notActivatedCode = [long]2150694920

# "This Windows has no BitLocker WMI provider": WBEM_E_INVALID_NAMESPACE,
# WBEM_E_INVALID_CLASS, WBEM_E_NOT_SUPPORTED (HRESULT in the error id, or the
# NativeErrorCode of a CimException).
$unsupportedHresults = @('8004100E', '80041010', '8004100C')
$unsupportedCimCodes = @('InvalidNamespace', 'InvalidClass', 'NotSupported')

$guidPattern = '[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}'

$systemDrive = ([string]$env:SystemDrive).Trim().ToUpperInvariant()

# True when a failed volume query means that BitLocker is not available here.
function Test-Unsupported {
    param($ErrorRecord)
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        if ($unsupportedCimCodes -contains [string]$ex.NativeErrorCode) {
            return $true
        }
        if ($unsupportedHresults -contains ('{0:X8}' -f $ex.HResult)) {
            return $true
        }
        $ex = $ex.InnerException
    }
    $errorId = [string]$ErrorRecord.FullyQualifiedErrorId
    foreach ($code in $unsupportedHresults) {
        if ($errorId -match ('(?i)\b0x' + $code + '\b')) {
            return $true
        }
    }
    return $false
}

# Whole number from a CIM value, or $Default.
function Get-Number {
    param($Value, [long]$Default)
    $n = [long]0
    if (($null -ne $Value) -and [long]::TryParse([string]$Value, [System.Globalization.NumberStyles]::Integer, $invariant, [ref]$n)) {
        return $n
    }
    return $Default
}

# Result of a method of the volume, or $null when the call itself failed.
function Invoke-VolumeMethod {
    param($Volume, [string]$Name, [hashtable]$Arguments)
    try {
        if ($null -eq $Arguments) {
            return (Invoke-CimMethod -InputObject $Volume -MethodName $Name -ErrorAction Stop)
        }
        return (Invoke-CimMethod -InputObject $Volume -MethodName $Name -Arguments $Arguments -ErrorAction Stop)
    }
    catch {
        return $null
    }
}

# ReturnValue of a method result; -1 when there is no result.
function Get-ReturnCode {
    param($Result)
    if ($null -eq $Result) {
        return [long]-1
    }
    return (Get-Number $Result.ReturnValue -1)
}

function Get-ProtectionStatus {
    param($Volume)
    $r = Invoke-VolumeMethod -Volume $Volume -Name 'GetProtectionStatus'
    if ((Get-ReturnCode $r) -eq 0) {
        return (Get-Number $r.ProtectionStatus 0)
    }
    return (Get-Number $Volume.ProtectionStatus 0)
}

# ConversionStatus and EncryptionPercentage (0..100, -1 when unknown).
function Get-ConversionState {
    param($Volume)
    $r = Invoke-VolumeMethod -Volume $Volume -Name 'GetConversionStatus' -Arguments @{ PrecisionFactor = [uint32]0 }
    if ((Get-ReturnCode $r) -eq 0) {
        $percent = Get-Number $r.EncryptionPercentage -1
        if ($percent -gt 100) {
            $percent = 100
        }
        return [pscustomobject]@{ Status = (Get-Number $r.ConversionStatus 0); Percent = $percent }
    }
    return [pscustomobject]@{ Status = (Get-Number $Volume.ConversionStatus 0); Percent = [long]-1 }
}

# GetLockStatus 1 = locked; when the call fails, protection status 2 (unknown)
# is taken as locked, which is its usual cause.
function Test-Locked {
    param($Volume, [long]$Protection)
    $r = Invoke-VolumeMethod -Volume $Volume -Name 'GetLockStatus'
    if ((Get-ReturnCode $r) -eq 0) {
        return ((Get-Number $r.LockStatus 0) -eq 1)
    }
    return ($Protection -eq 2)
}

# Key protector IDs of one type (0 = all, 3 = numerical password), or $null
# when they cannot be listed. FVE_E_NOT_ACTIVATED means there are none.
function Get-ProtectorId {
    param($Volume, [uint32]$Type)
    $r = Invoke-VolumeMethod -Volume $Volume -Name 'GetKeyProtectors' -Arguments @{ KeyProtectorType = $Type }
    $code = Get-ReturnCode $r
    if ($code -eq $notActivatedCode) {
        return , ([string[]]@())
    }
    if ($code -ne 0) {
        return $null
    }
    $ids = New-Object System.Collections.Generic.List[string]
    foreach ($id in @($r.VolumeKeyProtectorID)) {
        $text = ([string]$id).Trim()
        if ($text.Length -gt 0) {
            $ids.Add($text)
        }
    }
    return , $ids.ToArray()
}

# The protector ID without braces, in upper case; '' when it is not a GUID.
function Get-ProtectorGuid {
    param([string]$ProtectorId)
    $m = [regex]::Match($ProtectorId, $guidPattern)
    if ($m.Success) {
        return $m.Value.ToUpperInvariant()
    }
    return ''
}

# Protector GUIDs (upper case) named by event 845 in the BitLocker management
# log. Empty when the log is missing, empty or cannot be read: the hint is
# best effort and never fails the tool.
function Get-BackedUpProtector {
    $found = @{}
    try {
        $records = @(Get-WinEvent -FilterHashtable @{ LogName = $managementLog; Id = $backupEventId } -ErrorAction Stop)
        foreach ($record in $records) {
            foreach ($property in @($record.Properties)) {
                if ($null -eq $property) {
                    continue
                }
                foreach ($m in [regex]::Matches([string]$property.Value, $guidPattern)) {
                    $found[$m.Value.ToUpperInvariant()] = $true
                }
            }
        }
    }
    catch {
        return @{}
    }
    return $found
}

$unsupported = $false
$volumes = @()
try {
    $volumes = @(Get-CimInstance -Namespace $namespace -ClassName Win32_EncryptableVolume -ErrorAction Stop)
}
catch {
    if (-not (Test-Unsupported $_)) {
        throw
    }
    $unsupported = $true
}

$sections = New-Object System.Collections.Generic.List[object]
$volumeCount = 0
$waitingCount = 0
$keyCount = 0
$missingCount = 0
$lockedCount = 0
$unreadableCount = 0
$backedUpCount = 0

try {
    # ---- the BitLocker volumes, in display order (nothing secret yet)
    $candidates = New-Object System.Collections.Generic.List[object]
    $index = 0
    foreach ($v in $volumes) {
        $index++
        $type = Get-Number $v.VolumeType -1
        if ($type -eq 2) {
            continue
        }
        $protection = Get-ProtectionStatus $v
        $conversion = Get-ConversionState $v
        if (($protection -eq 0) -and ($conversion.Status -eq 0)) {
            continue
        }
        $letter = ([string]$v.DriveLetter).Trim().ToUpperInvariant()
        if ($letter -notmatch '^[A-Z]:$') {
            $letter = ''
        }
        $isOs = ($type -eq 0) -or (($type -lt 0) -and ($letter.Length -gt 0) -and ($letter -eq $systemDrive))
        $sectionId = 'no_letter'
        $order = '2' + $index.ToString('D4', $invariant)
        if ($letter.Length -gt 0) {
            $sectionId = 'data_volume'
            $order = '1' + $letter
            if ($isOs) {
                $sectionId = 'os_volume'
                $order = '0' + $letter
            }
        }
        $candidates.Add([pscustomobject]@{
                Volume      = $v
                Letter      = $letter
                SectionId   = $sectionId
                Order       = $order
                Protection  = $protection
                Conversion  = $conversion.Status
                Percent     = $conversion.Percent
                Locked      = $false
                Waiting     = $false
                PasswordIds = $null
            })
    }
    $ordered = @($candidates.ToArray() | Sort-Object -Property Order)

    # ---- lock state and key protectors (IDs only)
    $idCount = 0
    foreach ($c in $ordered) {
        $c.Locked = Test-Locked -Volume $c.Volume -Protection $c.Protection
        $all = Get-ProtectorId -Volume $c.Volume -Type 0
        $c.Waiting = ($c.Protection -eq 0) -and ($null -ne $all) -and ($all.Count -eq 0)
        if (-not $c.Waiting) {
            $c.PasswordIds = Get-ProtectorId -Volume $c.Volume -Type 3
            if ($null -ne $c.PasswordIds) {
                $idCount += $c.PasswordIds.Count
            }
        }
    }
    $backedUp = @{}
    if ($idCount -gt 0) {
        $backedUp = Get-BackedUpProtector
    }

    # ---- one table per volume; the passwords are read straight into their rows
    foreach ($c in $ordered) {
        $rows = New-Object System.Collections.Generic.List[object]

        $state = 'unknown'
        if ($c.Waiting) {
            $state = 'waiting'
        }
        elseif ($c.Locked) {
            $state = 'locked'
        }
        elseif ($c.Conversion -eq 2) {
            $state = 'encrypting'
        }
        elseif ($c.Conversion -eq 4) {
            $state = 'encryption-paused'
        }
        elseif ($c.Conversion -eq 3) {
            $state = 'decrypting'
        }
        elseif ($c.Conversion -eq 5) {
            $state = 'decryption-paused'
        }
        elseif ($c.Protection -eq 1) {
            $state = 'on'
        }
        elseif ($c.Protection -eq 0) {
            $state = 'suspended'
        }
        $rows.Add([ordered]@{ id = 'status'; code = $state })

        if ((-not $c.Locked) -and ($c.Percent -ge 0)) {
            if (($c.Conversion -eq 2) -or ($c.Conversion -eq 4)) {
                $rows.Add([ordered]@{ id = 'encrypt_progress'; value = ($c.Percent.ToString($invariant) + '%') })
            }
            elseif (($c.Conversion -eq 3) -or ($c.Conversion -eq 5)) {
                $rows.Add([ordered]@{ id = 'decrypt_progress'; value = ((100 - $c.Percent).ToString($invariant) + '%') })
            }
        }

        if ($c.Waiting) {
            $waitingCount++
            $rows.Add([ordered]@{ id = 'recovery_key'; code = 'not-needed' })
        }
        else {
            $volumeCount++
            if (($null -eq $c.PasswordIds) -and $c.Locked) {
                $lockedCount++
                $rows.Add([ordered]@{ id = 'recovery_key'; code = 'locked-key' })
            }
            elseif ($null -eq $c.PasswordIds) {
                $unreadableCount++
                $rows.Add([ordered]@{ id = 'recovery_key'; code = 'unreadable' })
            }
            elseif ($c.PasswordIds.Count -eq 0) {
                $missingCount++
                $rows.Add([ordered]@{ id = 'recovery_key'; code = 'no-key' })
            }
            else {
                $volumeLocked = $false
                $volumeUnreadable = $false
                foreach ($id in $c.PasswordIds) {
                    $guid = Get-ProtectorGuid $id
                    if ($guid.Length -gt 0) {
                        $rows.Add([ordered]@{ id = 'key_id'; value = $guid.Substring(0, 8) })
                    }
                    # The password: only operators and string methods touch it,
                    # and a failure keeps no message.
                    $failure = 'unreadable'
                    if ($c.Locked) {
                        $failure = 'locked-key'
                    }
                    $password = ''
                    try {
                        $r = Invoke-CimMethod -InputObject $c.Volume -MethodName 'GetKeyProtectorNumericalPassword' -Arguments @{ VolumeKeyProtectorID = $id } -ErrorAction Stop
                        $code = Get-ReturnCode $r
                        if ($code -eq $lockedVolumeCode) {
                            $failure = 'locked-key'
                        }
                        elseif ($code -eq 0) {
                            $raw = [string]$r.NumericalPassword
                            $digits = $raw -replace '[\s-]', ''
                            # [regex]::IsMatch, not -match: that would keep the value in $Matches.
                            if (($digits.Length -eq 48) -and [regex]::IsMatch($raw, '^[\s0-9-]+$')) {
                                $groups = New-Object 'string[]' 8
                                for ($g = 0; $g -lt 8; $g++) {
                                    $groups[$g] = $digits.Substring($g * 6, 6)
                                }
                                $password = $groups -join '-'
                            }
                            else {
                                $failure = 'unreadable'
                            }
                            $raw = $null
                            $digits = $null
                        }
                        $r = $null
                    }
                    catch {
                        $password = ''
                        $r = $null
                    }
                    if ($password.Length -gt 0) {
                        $keyCount++
                        $rows.Add([ordered]@{ id = 'recovery_key'; value = $password; secret = $true })
                    }
                    else {
                        $rows.Add([ordered]@{ id = 'recovery_key'; code = $failure })
                        if ($failure -eq 'locked-key') {
                            $volumeLocked = $true
                        }
                        else {
                            $volumeUnreadable = $true
                        }
                    }
                    $password = $null
                    if (($guid.Length -gt 0) -and $backedUp.ContainsKey($guid)) {
                        $backedUpCount++
                        $rows.Add([ordered]@{ id = 'backup'; code = 'backed-up' })
                    }
                }
                if ($volumeLocked) {
                    $lockedCount++
                }
                if ($volumeUnreadable) {
                    $unreadableCount++
                }
            }
        }

        $section = [ordered]@{ id = $c.SectionId }
        if ($c.Letter.Length -gt 0) {
            $section['name'] = $c.Letter
        }
        $section['rows'] = $rows.ToArray()
        $sections.Add($section)
    }
}
catch {
    # The original message could quote a value that was being handled.
    throw ('Could not read the BitLocker recovery keys ({0} at line {1})' -f $_.Exception.GetType().Name, $_.InvocationInfo.ScriptLineNumber)
}

$result = 'found'
if ($unsupported) {
    $result = 'unsupported'
}
elseif (($volumeCount -eq 0) -and ($waitingCount -eq 0)) {
    $result = 'none'
}
elseif ($volumeCount -eq 0) {
    $result = 'waiting'
}
elseif ($missingCount -gt 0) {
    $result = 'no-recovery-key'
}
elseif ($lockedCount -gt 0) {
    $result = 'locked'
}
elseif ($unreadableCount -gt 0) {
    $result = 'partial'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{
        volume_count     = $volumeCount
        waiting_count    = $waitingCount
        key_count        = $keyCount
        missing_count    = $missingCount
        locked_count     = $lockedCount
        unreadable_count = $unreadableCount
        backed_up_count  = $backedUpCount
    }
    sections = $sections.ToArray()
}
