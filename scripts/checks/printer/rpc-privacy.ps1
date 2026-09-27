# Check whether this machine shares any printers before discussing the 0x11b host setting.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$printers = $null
try { $printers = @(Get-Printer -ErrorAction Stop | Where-Object { $_.Shared -eq $true }) }
catch {
    [pscustomobject]@{ result = 'unknown'; facts = [ordered]@{} }
    return
}
if ($printers.Count -eq 0) {
    [pscustomobject]@{ result = 'not-host'; facts = [ordered]@{} }
    return
}

$key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Print'
$value = $null
# No key or no value means Windows' default.
try { $value = (Get-ItemProperty -LiteralPath $key -Name 'RpcAuthnLevelPrivacyEnabled' -ErrorAction Stop).RpcAuthnLevelPrivacyEnabled }
catch [System.Management.Automation.ItemNotFoundException] { $value = $null }
catch [System.Management.Automation.PSArgumentException] { $value = $null }

$code = if ($null -ne $value -and [int]$value -eq 0) { 'relaxed' } else { 'secure' }
[pscustomobject]@{ result = $code; facts = [ordered]@{ shared_printers = $printers.Count } }
