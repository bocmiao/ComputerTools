# Tool: system.win11-readiness (info)
# Can this Windows 10 PC be upgraded to Windows 11 for free? Read-only.
# 1. Windows' own verdict. The Microsoft Compatibility Appraiser (scheduled task
#    \Microsoft\Windows\Application Experience\Microsoft Compatibility
#    Appraiser) writes, for each Windows 11 release it assessed,
#    HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\
#    TargetVersionUpgradeExperienceIndicators\<release>, with:
#      UpgEx               Green, Yellow, Orange or Red (Red: the hardware blocks)
#      RedReason           "None", or the blocks separated by spaces: CpuFms
#                          (processor not supported), Tpm, UefiSecureBoot, ...
#      DestBuildNum        the build of that release
#      SystemDriveTooFull  1 when the system drive lacks free space
#    The release with the highest DestBuildNum is used. Blocks medkit does not
#    know are shown as they are, not guessed at.
# 2. What medkit reads itself, shown in any case: processor name, TPM, Secure
#    Boot, memory and the size of the system drive. Whether the processor is on
#    Microsoft's list is left to Windows' verdict (the list changes; medkit does
#    not keep a copy of it).
# Results: already (Windows 11) / server / ready (RedReason None) / blocked /
# not-assessed (no verdict to read).
# Facts: appraised (bool), target (build of the assessed release, or ""),
# blocks (number of blocks).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$indicatorsKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\TargetVersionUpgradeExperienceIndicators'
# RedReason words (lower case) -> row code.
$blockCodes = @{
    'cpufms'          = 'cpu'
    'cpumodel'        = 'cpu'
    'cpucores'        = 'cpu'
    'cpuspeed'        = 'cpu'
    'tpm'             = 'tpm'
    'uefisecureboot'  = 'secure_boot'
    'secureboot'      = 'secure_boot'
    'memory'          = 'memory'
    'ram'             = 'memory'
    'systemdrivesize' = 'disk'
    'storage'         = 'disk'
}

function Get-Text {
    param($Properties, [string]$Name)
    if ($null -eq $Properties) {
        return ''
    }
    $property = $Properties.PSObject.Properties[$Name]
    if (($null -eq $property) -or ($null -eq $property.Value)) {
        return ''
    }
    return ([string]$property.Value).Trim()
}

function Get-Number {
    param([string]$Text)
    $parsed = 0
    if ([int]::TryParse($Text, [ref]$parsed)) {
        return $parsed
    }
    return 0
}

$sections = New-Object System.Collections.Generic.List[object]

function Add-Section {
    param([string]$Id, [System.Collections.Generic.List[object]]$Rows)
    $list = $Rows.ToArray()
    if ($list.Count -gt 0) {
        $sections.Add([ordered]@{ id = $Id; rows = $list })
    }
}

function New-ValueRow {
    param([string]$Id, $Value)
    return [ordered]@{ id = $Id; value = $Value }
}

function New-CodeRow {
    param([string]$Id, [string]$Code)
    return [ordered]@{ id = $Id; code = $Code }
}

# ---- which Windows is this
$cv = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$build = Get-Number (Get-Text $cv 'CurrentBuild')
if ((Get-Text $cv 'InstallationType') -ieq 'Server') {
    [pscustomobject]@{ result = 'server'; facts = [ordered]@{ appraised = $false; target = ''; blocks = 0 } }
    return
}
if ($build -ge 22000) {
    [pscustomobject]@{ result = 'already'; facts = [ordered]@{ appraised = $false; target = ''; blocks = 0 } }
    return
}

# ---- Windows' own verdict
$verdict = $null
if (Test-Path -LiteralPath $indicatorsKey) {
    foreach ($release in @(Get-ChildItem -LiteralPath $indicatorsKey -ErrorAction SilentlyContinue)) {
        $values = Get-ItemProperty -LiteralPath $release.PSPath -ErrorAction SilentlyContinue
        $upgEx = Get-Text $values 'UpgEx'
        if ($upgEx.Length -eq 0) {
            continue
        }
        $candidate = [pscustomobject]@{
            Target    = Get-Text $values 'DestBuildNum'
            UpgEx     = $upgEx
            RedReason = Get-Text $values 'RedReason'
            DriveFull = (Get-Text $values 'SystemDriveTooFull') -eq '1'
        }
        if (($null -eq $verdict) -or ((Get-Number $candidate.Target) -gt (Get-Number $verdict.Target))) {
            $verdict = $candidate
        }
    }
}

$blockCount = 0
if ($null -ne $verdict) {
    $rows = New-Object System.Collections.Generic.List[object]
    if ($verdict.Target.Length -gt 0) {
        $rows.Add((New-ValueRow -Id 'target' -Value $verdict.Target))
    }
    $upgExCode = 'other'
    if (@('green', 'yellow', 'orange', 'red') -contains $verdict.UpgEx.ToLowerInvariant()) {
        $upgExCode = $verdict.UpgEx.ToLowerInvariant()
    }
    $rows.Add((New-CodeRow -Id 'verdict' -Code $upgExCode))
    $seen = @{}
    foreach ($word in @($verdict.RedReason -split '[\s,;]+' | Where-Object { $_.Length -gt 0 })) {
        if ($word -ieq 'None') {
            continue
        }
        $code = $blockCodes[$word.ToLowerInvariant()]
        if ($null -eq $code) {
            $rows.Add((New-ValueRow -Id 'block_other' -Value $word))
            $blockCount++
        }
        elseif (-not $seen.ContainsKey($code)) {
            $seen[$code] = $true
            $rows.Add((New-CodeRow -Id 'block' -Code $code))
            $blockCount++
        }
    }
    if ($verdict.DriveFull) {
        $rows.Add((New-CodeRow -Id 'drive_full' -Code 'yes'))
    }
    Add-Section -Id 'windows' -Rows $rows
}

# ---- what medkit reads itself
$rows = New-Object System.Collections.Generic.List[object]
$cpu = @(Get-CimInstance -ClassName Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1)
if ($cpu.Count -gt 0) {
    $name = ([string]$cpu[0].Name).Trim()
    if ($name.Length -gt 0) {
        $rows.Add((New-ValueRow -Id 'cpu' -Value $name))
    }
}

$tpmCode = 'unknown'
try {
    $tpm = @(Get-CimInstance -Namespace 'root\cimv2\Security\MicrosoftTpm' -ClassName Win32_Tpm -ErrorAction Stop)
    if ($tpm.Count -eq 0) {
        $tpmCode = 'none'
    }
    else {
        $spec = ([string]$tpm[0].SpecVersion).Trim()
        if ($spec.StartsWith('1.')) {
            $tpmCode = 'tpm12'
        }
        elseif ($spec.StartsWith('2.')) {
            $tpmCode = 'tpm2'
            if ((-not [bool]$tpm[0].IsEnabled_InitialValue) -or (-not [bool]$tpm[0].IsActivated_InitialValue)) {
                $tpmCode = 'tpm2-off'
            }
        }
    }
}
catch {
    $tpmCode = 'unknown'
}
$rows.Add((New-CodeRow -Id 'tpm' -Code $tpmCode))

$secureBootCode = 'unknown'
try {
    if ([bool](Confirm-SecureBootUEFI)) {
        $secureBootCode = 'on'
    }
    else {
        $secureBootCode = 'off'
    }
}
catch {
    # PlatformNotSupportedException, or STATUS_NOT_IMPLEMENTED (0xC0000002) in
    # the localized message: legacy BIOS (or CSM) boot.
    $ex = $_.Exception
    while ($null -ne $ex) {
        if ($ex -is [System.PlatformNotSupportedException]) {
            $secureBootCode = 'legacy'
        }
        $ex = $ex.InnerException
    }
    if ([string]$_.Exception.Message -match '0xC0000002') {
        $secureBootCode = 'legacy'
    }
}
$rows.Add((New-CodeRow -Id 'secure_boot' -Code $secureBootCode))

$computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction SilentlyContinue
if ($null -ne $computer) {
    $rows.Add((New-ValueRow -Id 'memory' -Value ('{0:0.#} GB' -f ([double]$computer.TotalPhysicalMemory / 1GB))))
}
$disk = Get-CimInstance -ClassName Win32_LogicalDisk -Filter ("DeviceID='{0}'" -f $env:SystemDrive) -ErrorAction SilentlyContinue
if ($null -ne $disk) {
    $rows.Add((New-ValueRow -Id 'system_disk' -Value ('{0:0.#} GB' -f ([double]$disk.Size / 1GB))))
}
Add-Section -Id 'hardware' -Rows $rows

$result = 'not-assessed'
if ($null -ne $verdict) {
    $result = 'blocked'
    if (($blockCount -eq 0) -and (-not $verdict.DriveFull) -and ($upgExCode -ne 'red')) {
        $result = 'ready'
    }
}
$target = ''
if ($null -ne $verdict) {
    $target = $verdict.Target
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{
        appraised = $null -ne $verdict
        target    = $target
        blocks    = $blockCount
    }
    sections = $sections.ToArray()
}
