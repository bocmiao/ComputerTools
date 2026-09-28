# Check: hardware.usb-storage
# The settings that make a plugged-in USB flash drive or external disk not
# show up, not get a drive letter or be read-only, and the USB disks plugged
# in now. (A disabled USB storage driver is hardware.usb-driver.)
# - Group Policy denies removable storage: Deny_All, or Deny_Read / Deny_Write
#   for removable disks ({53f5630d-b6bf-11d0-94f2-00a0c91efb8b}), under
#   Software\Policies\Microsoft\Windows\RemovableStorageDevices (the
#   machine's or the logged-in user's); or DenyRemovableDevices under
#   HKLM\SOFTWARE\Policies\Microsoft\Windows\DeviceInstall\Restrictions.
#   Offices set these; not changed here.
# - NoAutoMount = 1 under HKLM\SYSTEM\CurrentControlSet\Services\mountmgr
#   ("mountvol /N", diskpart "automount disable"): new drives get no drive
#   letter (fix: hardware.usb-automount-on).
# - WriteProtect = 1 under
#   HKLM\SYSTEM\CurrentControlSet\Control\StorageDevicePolicies: USB drives
#   are read-only, "The disk is write-protected" (fix:
#   hardware.usb-write-protect-off).
# - NoDrives / NoViewOnDrive (bit masks, bit 0 = A) under
#   Software\Microsoft\Windows\CurrentVersion\Policies\Explorer, the user's or
#   the machine's: drives hidden in This PC, or not openable. A and B (the
#   floppy drives old tweaks hide) do not count, nor hidden letters no USB
#   drive has while USB drives with letters are plugged in. Not changed here.
# - The USB disks plugged in now (Get-Disk: bus USB, SD or MMC; card readers
#   with no card in them, size 0, do not count): one that is offline, or has
#   no partition with a drive letter (none at all: not partitioned, RAW).
#   Names and volume labels are not read.
# Read-only. Result codes (in this order): policy / no-automount /
# write-protect / hidden-drives / no-letter / none (no USB disk is plugged
# in, or Windows does not see it) / ok / settings-ok (the disks could not be
# listed; the settings are fine).
# Facts: policy (all, read, write, install), letters (hidden drive letters,
# "E, F"), usb (USB disks seen), noletter (of them, how many have no drive
# letter), drives (the USB drives' letters, "E, F").

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}
$user = $UserHive.TrimEnd('\')

# A DWORD value, or $null when the key or the value is not there.
function Get-Dword {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop
    }
    catch {
        return $null
    }
    $value = $item.$Name
    if ($value -is [int]) {
        return $value
    }
    return $null
}

$denied = New-Object System.Collections.Generic.List[string]
foreach ($root in @('HKLM:', $user)) {
    $base = $root + '\Software\Policies\Microsoft\Windows\RemovableStorageDevices'
    $disks = $base + '\{53f5630d-b6bf-11d0-94f2-00a0c91efb8b}'
    foreach ($check in @(@($base, 'Deny_All', 'all'), @($disks, 'Deny_Read', 'read'), @($disks, 'Deny_Write', 'write'))) {
        if (((Get-Dword $check[0] $check[1]) -eq 1) -and -not $denied.Contains($check[2])) {
            $denied.Add($check[2])
        }
    }
}
if ((Get-Dword 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeviceInstall\Restrictions' 'DenyRemovableDevices') -eq 1) {
    $denied.Add('install')
}

$mask = 0
foreach ($root in @($user, 'HKLM:')) {
    foreach ($name in @('NoDrives', 'NoViewOnDrive')) {
        $value = Get-Dword ($root + '\Software\Microsoft\Windows\CurrentVersion\Policies\Explorer') $name
        if ($null -ne $value) {
            $mask = $mask -bor $value
        }
    }
}
$hidden = New-Object System.Collections.Generic.List[string]
for ($i = 2; $i -lt 26; $i++) {
    if (($mask -band (1 -shl $i)) -ne 0) {
        $hidden.Add([string][char](65 + $i))
    }
}

# The USB disks plugged in now; $null when they cannot be listed
$usb = $null
$noLetter = 0
$drives = New-Object System.Collections.Generic.List[string]
try {
    # Size 0: a card reader with no card in it
    $found = @(Get-Disk -ErrorAction Stop | Where-Object {
            (@('USB', 'SD', 'MMC', '7', '12', '13') -contains [string]$_.BusType) -and ([uint64]$_.Size -gt 0)
        })
    foreach ($disk in $found) {
        $letters = @()
        if (-not $disk.IsOffline) {
            $letters = @(Get-Partition -DiskNumber $disk.Number -ErrorAction SilentlyContinue |
                    Where-Object { ([string]$_.DriveLetter) -match '^[A-Za-z]$' } |
                    ForEach-Object { [string]$_.DriveLetter })
        }
        if ($letters.Count -eq 0) {
            $noLetter++
        }
        foreach ($letter in $letters) {
            $drives.Add($letter.ToUpperInvariant())
        }
    }
    $usb = $found.Count
}
catch {
    Write-Verbose ('The disks could not be listed: {0}' -f $_.Exception.Message)
}

# Hidden letters matter unless USB drives with letters are plugged in and none of them is hidden
$hiddenMatters = $hidden.Count -gt 0
if ($hiddenMatters -and ($drives.Count -gt 0)) {
    $hiddenMatters = @($drives | Where-Object { $hidden.Contains($_) }).Count -gt 0
}

$result = 'settings-ok'
if ($denied.Count -gt 0) {
    $result = 'policy'
}
elseif ((Get-Dword 'HKLM:\SYSTEM\CurrentControlSet\Services\mountmgr' 'NoAutoMount') -eq 1) {
    $result = 'no-automount'
}
elseif ((Get-Dword 'HKLM:\SYSTEM\CurrentControlSet\Control\StorageDevicePolicies' 'WriteProtect') -eq 1) {
    $result = 'write-protect'
}
elseif ($hiddenMatters) {
    $result = 'hidden-drives'
}
elseif ($null -ne $usb) {
    if ($noLetter -gt 0) {
        $result = 'no-letter'
    }
    elseif ($usb -eq 0) {
        $result = 'none'
    }
    else {
        $result = 'ok'
    }
}

$usbText = ''
if ($null -ne $usb) {
    $usbText = $usb
}
[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        policy   = $denied.ToArray() -join ', '
        letters  = $hidden.ToArray() -join ', '
        usb      = $usbText
        noletter = $noLetter
        drives   = $drives.ToArray() -join ', '
    }
}
