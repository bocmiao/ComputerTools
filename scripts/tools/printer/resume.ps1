# Tool: printer.resume
# Puts printers back to work: unchecks "Use Printer Offline" (WorkOffline)
# and resumes queues that are paused ("Pause Printing", ExtendedPrinterStatus
# 8), on every printer that has either. Uses the WMI scripting API
# (SWbemLocator) with SeLoadDriverPrivilege enabled, which saving a
# Win32_Printer needs (Microsoft, "Win32_Printer class"), and the class's
# Resume method. Nothing else about the printers is changed; the jobs in the
# queues are left alone (they print once the printer is back).
# Printer names are not output.
# Result codes: done (facts: fixed) / partial (facts: fixed, left) / none
# (no printer was offline or paused) / failed (facts: left).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$locator = New-Object -ComObject 'WbemScripting.SWbemLocator'
$wmi = $locator.ConnectServer('.', 'root\cimv2')
$null = $wmi.Security_.Privileges.AddAsString('SeLoadDriverPrivilege', $true)

$fixed = 0
$left = 0
foreach ($printer in @($wmi.ExecQuery('SELECT * FROM Win32_Printer'))) {
    if ($null -eq $printer) {
        continue
    }
    $offline = $printer.Properties_.Item('WorkOffline').Value -eq $true
    $paused = ([int]$printer.Properties_.Item('ExtendedPrinterStatus').Value) -eq 8
    if (-not ($offline -or $paused)) {
        continue
    }
    try {
        if ($offline) {
            $printer.Properties_.Item('WorkOffline').Value = $false
            $null = $printer.Put_()
        }
        if ($paused) {
            $out = $printer.ExecMethod_('Resume')
            $code = [int]$out.Properties_.Item('ReturnValue').Value
            if ($code -ne 0) {
                throw ('Resume returned {0}' -f $code)
            }
        }
        $fixed++
    }
    catch {
        Write-Verbose ('A printer could not be put back to work: {0}' -f $_.Exception.Message)
        $left++
    }
}

$result = 'none'
if (($fixed -gt 0) -and ($left -gt 0)) {
    $result = 'partial'
}
elseif ($fixed -gt 0) {
    $result = 'done'
}
elseif ($left -gt 0) {
    $result = 'failed'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ fixed = $fixed; left = $left }
}
