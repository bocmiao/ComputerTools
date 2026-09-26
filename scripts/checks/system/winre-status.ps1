# Check: system.winre-status
# Is the Windows Recovery Environment (WinRE) enabled?
# Reads %windir%\System32\Recovery\ReAgent.xml, the configuration file that
# reagentc.exe maintains. The text output of "reagentc /info" is localized, so it
# is not parsed.
#   <WindowsRE version="2.0">
#     <WinreLocation path="\Recovery\WindowsRE" id="0" offset="..." guid="{...}"/>
#     <InstallState state="1"/>      1 = enabled, 0 = disabled
# Microsoft does not document this file; the meaning of InstallState is taken from
# community tools (OSDeploy, Intune remediation scripts) and still needs to be
# verified on real machines (see the YAML).
# Read-only. Needs administrator rights (the Recovery folder is restricted).
# Result codes: enabled / disabled.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$path = Join-Path $systemDir 'Recovery\ReAgent.xml'
if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw ('ReAgent.xml was not found at {0}' -f $path)
}

$settings = New-Object System.Xml.XmlReaderSettings
$settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
$settings.XmlResolver = $null
$reader = [System.Xml.XmlReader]::Create($path, $settings)
try {
    $doc = New-Object System.Xml.XmlDocument
    $doc.Load($reader)
}
finally {
    $reader.Close()
}

$root = $doc.DocumentElement
if (($null -eq $root) -or ($root.LocalName -ne 'WindowsRE')) {
    throw 'ReAgent.xml does not have the expected WindowsRE root element'
}

$installNode = $root.SelectSingleNode("*[local-name()='InstallState']")
if ($null -eq $installNode) {
    throw 'ReAgent.xml has no InstallState element'
}
$state = ([string]$installNode.GetAttribute('state')).Trim()

$location = ''
$locationNode = $root.SelectSingleNode("*[local-name()='WinreLocation']")
if ($null -ne $locationNode) {
    $location = ([string]$locationNode.GetAttribute('path')).Trim()
}

if ($state -eq '1') {
    $result = 'enabled'
}
elseif ($state -eq '0') {
    $result = 'disabled'
}
else {
    throw ("Unexpected InstallState value '{0}' in ReAgent.xml" -f $state)
}

$facts = [ordered]@{
    install_state = $state
}
if ($location.Length -gt 0) {
    $facts['winre_location'] = $location
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
