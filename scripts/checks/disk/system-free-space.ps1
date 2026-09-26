# Check: disk.system-free-space
# Free and total space of the system drive ($env:SystemDrive), from Win32_LogicalDisk.
# Read-only. Outputs one object: { result, facts }.
# Result codes: ok / low / critical (texts live in catalog/checks/disk/system-free-space.yaml).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$systemDrive = [string]$env:SystemDrive
if ($systemDrive -notmatch '^[A-Za-z]:$') {
    throw ("Unexpected SystemDrive value: '{0}'" -f $systemDrive)
}

$disk = Get-CimInstance -ClassName Win32_LogicalDisk -Filter ("DeviceID='{0}'" -f $systemDrive)
if ($null -eq $disk) {
    throw ('Logical disk {0} was not found' -f $systemDrive)
}

$size = [double]$disk.Size
$free = [double]$disk.FreeSpace
if ($size -le 0) {
    throw ('Logical disk {0} reports no size' -f $systemDrive)
}

$freePct = $free / $size * 100

# Thresholds are compared on the raw values, not on the rounded facts.
# Absolute space is what matters (a feature update needs about 20 GB); the
# percentage only matters on small disks, so a 2 TB drive with 250 GB free is fine.
#   critical: less than 5 GB, or less than 5 percent and less than 20 GB
#   low:      less than 20 GB, or less than 10 percent and less than 50 GB
$result = 'ok'
if (($free -lt 5GB) -or (($freePct -lt 5) -and ($free -lt 20GB))) {
    $result = 'critical'
}
elseif (($free -lt 20GB) -or (($freePct -lt 10) -and ($free -lt 50GB))) {
    $result = 'low'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        drive    = $systemDrive.Substring(0, 1).ToUpperInvariant()
        free_gb  = [math]::Round($free / 1GB, 1)
        total_gb = [math]::Round($size / 1GB, 1)
        free_pct = [math]::Round($freePct, 1)
    }
}
