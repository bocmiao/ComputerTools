# Feature: system.enable-winre -- run (and prepare)
# Turns WinRE back on with "reagentc /enable" (Windows' own tool: it moves
# Winre.wim to the recovery partition and points the boot configuration at it;
# Microsoft's own steps use it too). No files are deleted.
# -Prepare: returns before = { enabled }.
# Run: -Before is that JSON. Returns skipped (nothing changed) when the state
#   changed since, WinRE is already on, or Winre.wim is gone (checked again
#   here: the engine goes ahead when the verify check could not run).
#   Otherwise runs reagentc /enable and reads ReAgent.xml back: it must say
#   enabled, or the script throws (the engine then runs the undo script).

[CmdletBinding()]
param(
    [bool]$Prepare = $false,
    [string]$Before = ''
)

$ErrorActionPreference = 'Stop'

# ---- shared block winre-state: identical in checks/system/winre-status.ps1, checks/system/winre-image.ps1 and features/system/winre-*.ps1 (medkit-data check compares them) ----
# The Windows Recovery Environment (WinRE) as reagentc.exe keeps it in
# %windir%\System32\Recovery\ReAgent.xml (the text output of "reagentc /info"
# is localized, so it is not parsed):
#   <WindowsRE version="2.0">
#     <WinreLocation path="\Recovery\WindowsRE" id="0" offset="..." guid="{...}"/>
#     <InstallState state="1"/>      1 = enabled, 0 = disabled
# Microsoft does not document this file; community tools read it the same way.
# While WinRE is disabled its image, Winre.wim (hidden), waits in the same
# folder; without it "reagentc /enable" fails ("The Windows RE image was not
# found"). reagentc.exe has no documented exit codes; 0 means it worked, and
# the state is read back from ReAgent.xml anyway.
# Needs administrator rights (the Recovery folder is restricted).

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$winreSystemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $winreSystemDir = Join-Path $env:windir 'Sysnative'
}
$winreRecoveryDir = Join-Path $winreSystemDir 'Recovery'
$reagentc = Join-Path $winreSystemDir 'reagentc.exe'

# Enabled ($true / $false), Location (the path where the image is when enabled,
# or ''), and ImagePresent (Winre.wim waits in the Recovery folder).
function Get-WinreState {
    $path = Join-Path $winreRecoveryDir 'ReAgent.xml'
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
    if (($state -ne '1') -and ($state -ne '0')) {
        throw ("Unexpected InstallState value '{0}' in ReAgent.xml" -f $state)
    }
    $location = ''
    $locationNode = $root.SelectSingleNode("*[local-name()='WinreLocation']")
    if ($null -ne $locationNode) {
        $location = ([string]$locationNode.GetAttribute('path')).Trim()
    }
    return [pscustomobject]@{
        Enabled      = ($state -eq '1')
        Location     = $location
        ImagePresent = (Test-Path -LiteralPath (Join-Path $winreRecoveryDir 'Winre.wim') -PathType Leaf)
    }
}

# Runs reagentc.exe (absolute path; its output is localized and not read).
# Returns the exit code.
function Invoke-Reagentc {
    param([string[]]$Arguments)
    $ErrorActionPreference = 'Continue'
    $null = & $reagentc @Arguments 2>&1
    return $LASTEXITCODE
}
# ---- end of shared block winre-state ----

$state = Get-WinreState
if ($Prepare) {
    return [pscustomobject]@{ before = [ordered]@{ enabled = $state.Enabled } }
}
$recorded = ConvertFrom-Json -InputObject $Before
if (([bool]$recorded.enabled -ne $state.Enabled) -or $state.Enabled -or (-not $state.ImagePresent)) {
    return [pscustomobject]@{ skipped = $true }
}
$exitCode = Invoke-Reagentc @('/enable')
if ($exitCode -ne 0) {
    throw ('reagentc /enable failed (exit code {0})' -f $exitCode)
}
if (-not (Get-WinreState).Enabled) {
    throw 'reagentc /enable reported success, but ReAgent.xml still says disabled'
}
[pscustomobject]@{ after = [ordered]@{ enabled = $true } }
