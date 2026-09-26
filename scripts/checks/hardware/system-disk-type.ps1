# Check: hardware.system-disk-type
# Is Windows installed on an SSD or on a spinning hard disk (HDD)?
# Follows the system drive to its physical disk:
#   Get-Partition -DriveLetter C -> Get-Disk -Number N -> Get-PhysicalDisk (DeviceId N)
# and reads MediaType (HDD / SSD / SCM / Unspecified) and BusType.
# Virtual disks (virtual machines) are reported as "virtual": whether the host
# stores them on an SSD is not something the user can change here.
# When MediaType is Unspecified:
#   - NVMe, SD, MMC, UFS and SCM buses are always flash;
#   - SpindleSpeed 0 means non-rotational media (MSFT_PhysicalDisk documentation);
#   - any other SpindleSpeed (including 0xFFFFFFFF, "speed unknown") is treated
#     as a spinning disk.
# A disk without SpindleSpeed throws, and the engine shows "unknown".
# Read-only. Result codes: ssd / hdd / virtual.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$mediaNames = @{ '0' = 'Unspecified'; '3' = 'HDD'; '4' = 'SSD'; '5' = 'SCM' }
$busNames = @{
    '0' = 'Unknown'; '1' = 'SCSI'; '2' = 'ATAPI'; '3' = 'ATA'; '4' = '1394'; '5' = 'SSA'
    '6' = 'Fibre Channel'; '7' = 'USB'; '8' = 'RAID'; '9' = 'iSCSI'; '10' = 'SAS'; '11' = 'SATA'
    '12' = 'SD'; '13' = 'MMC'; '14' = 'Virtual'; '15' = 'File Backed Virtual'
    '16' = 'Storage Spaces'; '17' = 'NVMe'; '18' = 'SCM'; '19' = 'UFS'
}
$flashBuses = @('NVMe', 'SD', 'MMC', 'UFS', 'SCM')

function ConvertTo-Name {
    param($Value, [hashtable]$Names)
    $text = [string]$Value
    if ($Names.ContainsKey($text)) {
        return $Names[$text]
    }
    return $text
}

$letter = ([string]$env:SystemDrive).TrimEnd(':')
if ($letter -notmatch '^[A-Za-z]$') {
    throw ("Unexpected SystemDrive value: '{0}'" -f $env:SystemDrive)
}

$partition = Get-Partition -DriveLetter $letter
$disk = Get-Disk -Number $partition.DiskNumber

$physical = @(Get-PhysicalDisk | Where-Object { [string]$_.DeviceId -eq [string]$disk.Number })
if ($physical.Count -eq 0) {
    $serial = ([string]$disk.SerialNumber).Trim()
    if ($serial.Length -gt 0) {
        $physical = @(Get-PhysicalDisk | Where-Object { ([string]$_.SerialNumber).Trim() -eq $serial })
    }
}
if ($physical.Count -ne 1) {
    throw ('Cannot map disk {0} (bus {1}) to exactly one physical disk; found {2}' -f $disk.Number, [string]$disk.BusType, $physical.Count)
}
$pd = $physical[0]

$media = ConvertTo-Name -Value $pd.MediaType -Names $mediaNames
$bus = ConvertTo-Name -Value $pd.BusType -Names $busNames
$spindle = $null
if ($null -ne $pd.SpindleSpeed) {
    $spindle = [uint32]$pd.SpindleSpeed
}

$model = ([string]$pd.FriendlyName).Trim()
if ($model.Length -eq 0) {
    $model = ([string]$disk.FriendlyName).Trim()
}
$virtualBuses = @('Virtual', 'File Backed Virtual')
# Hyper-V / Azure "Msft Virtual Disk", VMware "Virtual disk", VirtualBox, QEMU, virtio
$virtualModel = $model -match '(?i)virtual|vbox|qemu|virtio'

$type = $null
if (($virtualBuses -contains $bus) -or $virtualModel) {
    $type = 'virtual'
}
elseif (($media -eq 'SSD') -or ($media -eq 'SCM')) {
    $type = 'ssd'
}
elseif ($media -eq 'HDD') {
    $type = 'hdd'
}
elseif ($flashBuses -contains $bus) {
    $type = 'ssd'
}
elseif ($null -eq $spindle) {
    throw ('Cannot tell the media type of the system disk: MediaType {0}, BusType {1}, no SpindleSpeed' -f $media, $bus)
}
elseif ($spindle -eq 0) {
    $type = 'ssd'
}
else {
    $type = 'hdd'
}

[pscustomobject]@{
    result = $type
    facts  = [ordered]@{
        media_type  = $type.ToUpperInvariant()
        bus_type    = $bus
        model       = $model
        size_gb     = [math]::Round(([double]$pd.Size) / 1GB, 0)
        disk_number = [int]$disk.Number
    }
}
