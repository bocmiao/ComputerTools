# Check: hardware.usb-driver
# Is the USB mass storage driver (USBSTOR) disabled? With Start = 4 under
# HKLM\SYSTEM\CurrentControlSet\Services\USBSTOR most USB flash drives, card
# readers and external disks do nothing when plugged in, and do not show up in
# This PC (Microsoft's "Prevent users from connecting to a USB storage device"
# does exactly this; so do "USB lock" and "optimizer" tools). Windows default:
# 3 (started when such a device is plugged in). Fast USB 3 disks that use UAS
# (the uaspstor driver) are not affected. The key is only there once a USB
# storage device has been installed.
# Read-only. Result codes: missing (no USBSTOR service) / disabled (fix:
# hardware.enable-usb-storage) / ok. Facts: start.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

try {
    $start = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\USBSTOR' -Name 'Start' -ErrorAction Stop).Start
}
catch {
    [pscustomobject]@{ result = 'missing'; facts = [ordered]@{ start = '' } }
    return
}

$result = 'ok'
if ([int]$start -eq 4) {
    $result = 'disabled'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ start = [int]$start }
}
