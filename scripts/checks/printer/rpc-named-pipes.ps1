# Check the client-side RPC transport policy used for printer connections.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$key = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Printers\RPC'
$value = $null
try { $value = (Get-ItemProperty -LiteralPath $key -Name 'RpcUseNamedPipeProtocol' -ErrorAction Stop).RpcUseNamedPipeProtocol }
catch [System.Management.Automation.ItemNotFoundException] { }
catch [System.Management.Automation.PSArgumentException] { }

$code = if ($null -ne $value -and [int]$value -eq 1) { 'enabled' } else { 'default' }
[pscustomobject]@{ result = $code; facts = [ordered]@{} }
