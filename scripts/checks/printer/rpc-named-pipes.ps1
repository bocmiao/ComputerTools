# Check the client-side RPC transport policy used for printer connections.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$key = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\Printers\RPC'
$value = $null
# No key or no value means Windows' default.
try { $value = (Get-ItemProperty -LiteralPath $key -Name 'RpcUseNamedPipeProtocol' -ErrorAction Stop).RpcUseNamedPipeProtocol }
catch [System.Management.Automation.ItemNotFoundException] { $value = $null }
catch [System.Management.Automation.PSArgumentException] { $value = $null }

$code = if ($null -ne $value -and [int]$value -eq 1) { 'enabled' } else { 'default' }
[pscustomobject]@{ result = $code; facts = [ordered]@{} }
