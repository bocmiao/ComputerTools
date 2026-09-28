# Check: printer.default-managed
# Does Windows pick the default printer by itself? With "Let Windows manage
# my default printer" on (the default since Windows 10 1511), the default is
# the printer used last on the current network, so documents can suddenly go
# to "Microsoft Print to PDF" or another printer. The setting is the DWORD
# LegacyDefaultPrinterMode under
# HKCU\Software\Microsoft\Windows NT\CurrentVersion\Windows: 1 = off (the
# default printer stays what the user chose), 0 or missing = on. Not a
# problem either way. Reads the signed-in user's hive (-UserHive).
# Result codes: managed (ok, with a hint and the fix printer.keep-default) /
# fixed (ok). Facts: legacy_mode (the value, '' when missing).

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($UserHive)) {
    $UserHive = 'HKCU:'
}

$key = $UserHive.TrimEnd('\') + '\Software\Microsoft\Windows NT\CurrentVersion\Windows'
$value = $null
try {
    $value = [int](Get-ItemProperty -LiteralPath $key -Name 'LegacyDefaultPrinterMode' -ErrorAction Stop).LegacyDefaultPrinterMode
}
catch {
    $value = $null
}

$facts = [ordered]@{ legacy_mode = '' }
if ($null -ne $value) {
    $facts.legacy_mode = $value
}

$result = 'managed'
if ($value -eq 1) {
    $result = 'fixed'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
