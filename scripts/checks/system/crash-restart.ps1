# Check: system.crash-restart
# Does Windows restart by itself right after a blue screen? That is the
# default (AutoReboot = 1 under HKLM\SYSTEM\CurrentControlSet\Control\
# CrashControl): the blue screen shows only for a moment and its stop code is
# easy to miss. With AutoReboot = 0 it stays until the power button is
# pressed (Microsoft, "Memory dump file options": AutoReboot REG_DWORD 0x1;
# changes need a restart). Not a problem either way: the tool "Recent blue
# screens" reads the stop codes from the event log. Read-only.
# Result codes: auto-restart (ok, with a hint and the fix
# system.bluescreen-stay) / stays (ok).
# Facts: auto_reboot (the DWORD value, '' when missing).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$crashKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\CrashControl'

function Get-Dword {
    param([string]$Name)
    try {
        $value = (Get-ItemProperty -LiteralPath $crashKey -Name $Name -ErrorAction Stop).$Name
        return [int]$value
    }
    catch {
        return $null
    }
}

$autoReboot = Get-Dword 'AutoReboot'
$facts = [ordered]@{ auto_reboot = '' }
if ($null -ne $autoReboot) {
    $facts.auto_reboot = $autoReboot
}

# Missing means the default, restart.
$result = 'auto-restart'
if ($autoReboot -eq 0) {
    $result = 'stays'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
