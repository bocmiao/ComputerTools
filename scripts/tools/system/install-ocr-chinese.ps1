# Tool: system.install-ocr-chinese (action)
# Installs the Simplified Chinese text recognizer that the toolbox's
# "picture to text" uses (Windows.Media.Ocr): the Windows capability
# Language.OCR~~~zh-CN~0.0.1.0. It needs the basic component of the same
# language, Language.Basic~~~zh-CN~0.0.1.0 (Microsoft, "Language and region
# Features on Demand (FOD)"), which is added first when it is missing.
# Microsoft's PowerToys Text Extractor documentation installs recognizers as
# Windows capabilities too (Add-WindowsCapability); here Dism.exe /Online
# /Add-Capability does it, started hidden the way the .NET Framework 3.5 tool
# starts Dism.exe. Windows downloads the files from Windows Update.
# What keeps the download from working is read, never changed (policies:
# plan section 5), as for .NET Framework 3.5:
#   HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU UseWUServer = 1
#     (updates from an organization's update server), unless
#   HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Servicing
#     RepairContentServerSource = 2 (optional features straight from Windows
#     Update);
#   the Windows Update service (wuauserv) set to Disabled: Dism.exe is not run.
# Result codes: already / installed / restart / service-disabled /
# update-server / download-failed / policy / corrupt / missing (Windows has
# no such capability) / failed / slow (Dism.exe still running after 9
# minutes: it is left to finish) / unsupported (Windows Server). Facts: code
# (Dism.exe's exit code, 0x...).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$ocrName = 'Language.OCR~~~zh-CN~0.0.1.0'
$basicName = 'Language.Basic~~~zh-CN~0.0.1.0'

# Dism.exe gets 9 minutes in all: the tool times out after 10.
$deadline = [DateTime]::UtcNow.AddMinutes(9)

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32", and
# the 32-bit Dism.exe cannot service 64-bit Windows.
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}

# Windows Server is left alone; ProductType 1 is a workstation.
function Test-Workstation {
    try {
        return ([int](Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop).ProductType -eq 1)
    }
    catch {
        Write-Verbose ('could not read the product type: ' + $_.Exception.Message)
        return $true
    }
}

# The state of a capability: installed, pending (a restart finishes it),
# absent (not installed), missing (Windows has no capability by that name)
# or unknown (it could not be read).
function Get-CapabilityState {
    param([string]$Name)
    try {
        $capability = Get-WindowsCapability -Online -Name $Name -ErrorAction Stop
    }
    catch {
        Write-Verbose ('could not read the capability: ' + $_.Exception.Message)
        return 'unknown'
    }
    if ($null -eq $capability) {
        return 'missing'
    }
    switch ([string]@($capability)[0].State) {
        'Installed' {
            return 'installed'
        }
        'InstallPending' {
            return 'pending'
        }
    }
    return 'absent'
}

# Adds a capability: Dism.exe's exit code, or $null when it is still running
# at the deadline (it is left to finish).
function Add-Capability {
    param([string]$Name)
    $arguments = '/Online /Add-Capability /CapabilityName:' + $Name + ' /NoRestart /Quiet'
    $start = New-Object Diagnostics.ProcessStartInfo((Join-Path $systemDir 'Dism.exe'), $arguments)
    $start.UseShellExecute = $true
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.WorkingDirectory = $systemDir
    $process = [Diagnostics.Process]::Start($start)
    $wait = [int][Math]::Max(1000, ($deadline - [DateTime]::UtcNow).TotalMilliseconds)
    if (-not $process.WaitForExit($wait)) {
        return $null
    }
    return [int]$process.ExitCode
}

# An exit code as the result code it leads to, the same way as for .NET
# Framework 3.5 (Microsoft, ".NET Framework 3.5 installation errors"):
# installed, restart, download (0x800F0906, 0x800F081F, 0x800F0950,
# 0x800F0954 and the Windows Update and network errors 0x8024xxxx,
# 0x80072xxx), policy (0x800F0907), service-disabled (0x80070422), corrupt
# (0x800F0831, 0x80073712), missing (0x800F080C) or failed.
function Get-AddResult {
    param([int]$Code)
    if ($Code -eq 0) {
        return 'installed'
    }
    if ($Code -eq 3010) {
        return 'restart'
    }
    $hex = '0x{0:X8}' -f $Code
    if (@('0x800F0906', '0x800F081F', '0x800F0950', '0x800F0954') -contains $hex) {
        return 'download'
    }
    if ($hex.StartsWith('0x8024') -or $hex.StartsWith('0x80072')) {
        return 'download'
    }
    switch ($hex) {
        '0x800F0907' {
            return 'policy'
        }
        '0x80070422' {
            return 'service-disabled'
        }
        '0x800F0831' {
            return 'corrupt'
        }
        '0x80073712' {
            return 'corrupt'
        }
        '0x800F080C' {
            return 'missing'
        }
    }
    return 'failed'
}

function Get-Value {
    param([string]$Path, [string]$Name)
    try {
        $item = Get-ItemProperty -LiteralPath $Path -ErrorAction Stop
    }
    catch {
        return $null
    }
    # A key without any values (an empty policy key) gives nothing.
    if ($null -eq $item) {
        return $null
    }
    $property = $item.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

# The value is exactly the number (a missing or text value is not).
function Test-Number {
    param($Value, [int64]$Number)
    if ($null -eq $Value) {
        return $false
    }
    try {
        return ([int64]$Value -eq $Number)
    }
    catch {
        return $false
    }
}

$facts = [ordered]@{ code = '' }

if (-not (Test-Workstation)) {
    [pscustomobject]@{ result = 'unsupported'; facts = $facts }
    return
}

$state = Get-CapabilityState $ocrName
if ($state -eq 'installed') {
    [pscustomobject]@{ result = 'already'; facts = $facts }
    return
}
if ($state -eq 'pending') {
    [pscustomobject]@{ result = 'restart'; facts = $facts }
    return
}
if ($state -eq 'missing') {
    [pscustomobject]@{ result = 'missing'; facts = $facts }
    return
}
if (Test-Number (Get-Value 'HKLM:\SYSTEM\CurrentControlSet\Services\wuauserv' 'Start') 4) {
    [pscustomobject]@{ result = 'service-disabled'; facts = $facts }
    return
}

$result = 'installed'
foreach ($name in @($basicName, $ocrName)) {
    if (($name -eq $basicName) -and (@('installed', 'pending') -contains (Get-CapabilityState $basicName))) {
        continue
    }
    $code = Add-Capability $name
    if ($null -eq $code) {
        $result = 'slow'
        break
    }
    $facts.code = '0x{0:X8}' -f $code
    $step = Get-AddResult $code
    if ($step -eq 'restart') {
        $result = 'restart'
        continue
    }
    if ($step -ne 'installed') {
        $result = $step
        break
    }
}

if ($result -eq 'download') {
    $updateServer = Test-Number (Get-Value 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU' 'UseWUServer') 1
    $straight = Test-Number (Get-Value 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Servicing' 'RepairContentServerSource') 2
    $result = 'download-failed'
    if ($updateServer -and (-not $straight)) {
        $result = 'update-server'
    }
}
elseif (($result -eq 'installed') -and ((Get-CapabilityState $ocrName) -eq 'pending')) {
    $result = 'restart'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
