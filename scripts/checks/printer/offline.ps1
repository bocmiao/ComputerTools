# Check: printer.offline
# Are printers offline or paused? From Win32_Printer (read-only; no
# administrator rights needed):
# - WorkOffline = TRUE: "Use Printer Offline" is checked in the print queue
#   window (Printer menu); jobs stay in the queue and never reach the printer.
# - ExtendedPrinterStatus = 8: "Pause Printing" is checked.
#   Both are fixed by the tool printer.resume and count on any queue.
# - PrinterStatus or ExtendedPrinterStatus = 7 (Offline) on a real printer:
#   Windows cannot reach it. Network printers (a shared printer on another
#   PC, or any port that is not USB, DOT4, LPT or COM) and USB ones get
#   different advice.
# Virtual printers (Microsoft Print to PDF, XPS Document Writer, OneNote, Fax,
# printers redirected into a remote session) are known by their port:
# PORTPROMPT:, nul:, SHRFAX:, XPSPort:, FILE:, TSnnn, or OneNote's own port.
# Printer names are often chosen by people and are not output.
# Result codes (in this order): work-offline / paused / offline-network /
# offline-usb / ok / none (no real printer is installed).
# Facts: printers (real printers), work_offline, paused, offline (real
# printers that are offline).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$virtualPortPattern = '(?i)^(PORTPROMPT:|nul:?|SHRFAX:|XPSPort:|FILE:|TS\d+|.*OneNote.*)$'
$usbPortPattern = '(?i)^(USB\d+|DOT4_\d+|LPT\d+:?|COM\d+:?)$'

$printers = @(Get-CimInstance -ClassName 'Win32_Printer')
$real = 0
$workOffline = 0
$paused = 0
$offlineNetwork = 0
$offlineUsb = 0
foreach ($printer in $printers) {
    if ($null -eq $printer) {
        continue
    }
    $port = ([string]$printer.PortName).Trim()
    $extended = 0
    if ($null -ne $printer.ExtendedPrinterStatus) {
        $extended = [int]$printer.ExtendedPrinterStatus
    }
    if ($printer.WorkOffline -eq $true) {
        $workOffline++
    }
    if ($extended -eq 8) {
        $paused++
    }
    if (($port -match $virtualPortPattern) -and ($printer.Network -ne $true)) {
        continue
    }
    $real++
    if (($extended -eq 7) -or ($printer.PrinterStatus -eq 7)) {
        if (($printer.Network -ne $true) -and ($port -match $usbPortPattern)) {
            $offlineUsb++
        }
        else {
            $offlineNetwork++
        }
    }
}

$result = 'ok'
if ($workOffline -gt 0) {
    $result = 'work-offline'
}
elseif ($paused -gt 0) {
    $result = 'paused'
}
elseif ($offlineNetwork -gt 0) {
    $result = 'offline-network'
}
elseif ($offlineUsb -gt 0) {
    $result = 'offline-usb'
}
elseif ($real -eq 0) {
    $result = 'none'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        printers     = $real
        work_offline = $workOffline
        paused       = $paused
        offline      = $offlineNetwork + $offlineUsb
    }
}
