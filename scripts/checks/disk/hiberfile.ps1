# Check: disk.hiberfile
# How much of the system drive the hibernation file (hiberfil.sys) takes, and
# whether medkit's "reduce the hibernation file" (disk.reduce-hiberfile) makes
# sense on this PC. Read-only. Outputs one object: { result, facts }.
# Result codes (texts live in catalog/checks/disk/hiberfile.yaml):
#   off      no hibernation file (hibernation is off, or this PC cannot hibernate)
#   custom   the size was set by hand (HiberFileSizePercent); left alone
#   reduced  already the reduced file (fast startup only)
#   battery  a full file on a PC with a battery (laptop, tablet, or a desktop
#            with a UPS that reports as a battery) or on a laptop by its
#            system type: hibernation keeps the work when the battery runs
#            out, so reducing is not offered
#   small    a full file, but reducing would free less than 1 GB
#   full     a full file on a PC without a battery: can be reduced
#   unknown  the kind of file could not be told
# The disk.reduce-hiberfile feature is judged by this check (verify): na
# results mean "not on this PC", ok means nothing to do, advice means it can
# be reduced.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block hiberfile-state: identical in checks/disk/hiberfile.ps1 and features/disk/reduce-hiberfile-detect.ps1, -run.ps1 and -undo.ps1 (medkit-data check compares them) ----
# The hibernation file as powercfg manages it, in
# HKLM\SYSTEM\CurrentControlSet\Control\Power:
#   HibernateEnabled      0 = hibernation off
#   HiberFileType         1 = reduced (fast startup only), 2 = full;
#                         missing = the default Windows picked
#   HiberFileSizePercent  a size set by hand, in percent of memory;
#                         0 or missing = Windows manages the size
# and the file itself, hiberfil.sys in the root of the system drive.
# Kind, in this order: off (no hibernation file), reduced (HiberFileType 1),
# custom (a size set by hand: powercfg treats it as full and cannot reduce it
# before the size is reset), full (HiberFileType 2), else told by the size of
# the file (Windows makes a full file 40 percent of memory and a reduced one 20
# percent), or unknown.
$powerKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Power'

function Get-PowerDword {
    param($Properties, [string]$Name)
    if ($null -eq $Properties) {
        return $null
    }
    $property = $Properties.PSObject.Properties[$Name]
    if (($null -eq $property) -or ($null -eq $property.Value)) {
        return $null
    }
    try {
        return [int64]$property.Value
    }
    catch {
        return $null
    }
}

function Get-HiberState {
    $properties = $null
    if (Test-Path -LiteralPath $powerKey) {
        $properties = Get-ItemProperty -LiteralPath $powerKey
    }
    $size = [double]0
    $file = Get-Item -LiteralPath ($env:SystemDrive + '\hiberfil.sys') -Force -ErrorAction SilentlyContinue
    if (($null -ne $file) -and (-not $file.PSIsContainer)) {
        $size = [double]$file.Length
    }
    $computer = Get-CimInstance -ClassName Win32_ComputerSystem
    $memory = [double]$computer.TotalPhysicalMemory
    $state = [pscustomobject]@{
        Kind    = 'unknown'
        Enabled = Get-PowerDword $properties 'HibernateEnabled'
        Type    = Get-PowerDword $properties 'HiberFileType'
        Percent = Get-PowerDword $properties 'HiberFileSizePercent'
        Size    = $size
        Memory  = $memory
        # PCSystemType 2 = mobile (a laptop, even with the battery taken out)
        Mobile  = ([int]$computer.PCSystemType -eq 2)
    }
    if (($state.Enabled -eq 0) -or ($size -le 0)) {
        $state.Kind = 'off'
    }
    elseif ($state.Type -eq 1) {
        $state.Kind = 'reduced'
    }
    elseif (($null -ne $state.Percent) -and ($state.Percent -gt 0)) {
        $state.Kind = 'custom'
    }
    elseif ($state.Type -eq 2) {
        $state.Kind = 'full'
    }
    elseif ($memory -gt 0) {
        if (($size / $memory) -lt 0.3) {
            $state.Kind = 'reduced'
        }
        else {
            $state.Kind = 'full'
        }
    }
    return $state
}

# For a full file: why reducing is not offered. 'battery': a PC with a battery
# (laptop, tablet, a UPS that reports as one) or a laptop by its system type:
# hibernation keeps the work when the battery runs out. 'small': reducing would
# free less than 1 GB (the reduced file is 20 percent of memory). '' when the
# file can be reduced.
function Get-HiberBlock {
    param($State)
    if ($State.Mobile -or (@(Get-CimInstance -ClassName Win32_Battery).Count -gt 0)) {
        return 'battery'
    }
    if (($State.Size - ($State.Memory * 0.2)) -lt 1GB) {
        return 'small'
    }
    return ''
}
# ---- end of shared block hiberfile-state ----

$state = Get-HiberState
$result = $state.Kind
if ($result -eq 'full') {
    $block = Get-HiberBlock $state
    if ($block.Length -gt 0) {
        $result = $block
    }
}
# What reducing frees: the reduced file is 20 percent of memory.
$save = [math]::Max([double]0, $state.Size - ($state.Memory * 0.2))

$percent = 0
if ($null -ne $state.Percent) {
    $percent = $state.Percent
}
[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        size_gb   = [math]::Round($state.Size / 1GB, 1)
        memory_gb = [math]::Round($state.Memory / 1GB, 1)
        save_gb   = [math]::Round($save / 1GB, 1)
        percent   = $percent
    }
}
