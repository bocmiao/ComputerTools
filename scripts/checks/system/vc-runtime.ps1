# Check: system.vc-runtime
# Is the Microsoft Visual C++ Redistributable (v14) there? Many programs and
# games built with Visual C++ do not start without it ("VCRUNTIME140.dll was
# not found", "MSVCP140.dll is missing", 0xc000007b when only one of the
# 32-bit and 64-bit ones is there). See the shared block for what is read.
# Read-only. Result codes (in this order):
#   missing  a needed architecture is not installed (fix: the tool
#            system.install-vc-runtime)
#   broken   installed, but its files are gone ("cleaner" tools delete them)
#   old      installed, older than 14.40
#   ok
# Facts: x64, x86 (installed versions, '' when not installed or not needed),
# versions (the installed ones: "x64 14.51.36247.00, x86 14.51.36247.00"),
# missing, broken, old (the architectures, comma separated).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block vc-runtime: identical in checks/system/vc-runtime.ps1 and tools/system/install-vc-runtime.ps1 (medkit-data check compares them) ----
# The Microsoft Visual C++ Redistributable v14 (Visual Studio 2015 to 2026).
# 32-bit programs need the x86 one, 64-bit programs the x64 one: 64-bit
# Windows needs both (on ARM64 Windows the x64 package also brings the ARM64
# files), 32-bit Windows only x86.
function Get-VcArchitecture {
    if ([Environment]::Is64BitOperatingSystem) {
        return @('x64', 'x86')
    }
    return @('x86')
}

# One architecture: the installed version ('' when it is not installed) from
# HKLM\SOFTWARE[\WOW6432Node]\Microsoft\VisualStudio\14.0\VC\Runtimes\<arch>
# (Installed = 1, Version "v14.44.35211.00"; Microsoft, "Redistribute Visual
# C++ files"), and whether the files programs load are there (vcruntime140.dll
# and msvcp140.dll, in SysWOW64 for x86 on 64-bit Windows).
function Get-VcRuntime {
    param([string]$Arch)
    $version = ''
    foreach ($root in @('HKLM:\SOFTWARE\Microsoft', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft')) {
        try {
            $key = Get-ItemProperty -LiteralPath ($root + '\VisualStudio\14.0\VC\Runtimes\' + $Arch) -ErrorAction Stop
        }
        catch {
            continue
        }
        if ($key.Installed -ne 1) {
            continue
        }
        $text = ''
        if ($key.Version -is [string]) {
            $text = $key.Version.Trim().TrimStart('v', 'V')
        }
        elseif ($null -ne $key.Major) {
            $text = '{0}.{1}.{2}' -f $key.Major, $key.Minor, $key.Bld
        }
        if ($text.Length -eq 0) {
            $text = '14.0'
        }
        $version = $text
        break
    }
    $windows = ([string]$env:SystemRoot).TrimEnd('\')
    $folder = $windows + '\System32'
    if (($Arch -eq 'x86') -and [Environment]::Is64BitOperatingSystem) {
        $folder = $windows + '\SysWOW64'
    }
    $files = $true
    foreach ($name in @('vcruntime140.dll', 'msvcp140.dll')) {
        if (-not (Test-Path -LiteralPath ($folder + '\' + $name) -PathType Leaf)) {
            $files = $false
        }
    }
    return [pscustomobject]@{ arch = $Arch; version = $version; files = $files }
}

# Older than 14.40 (Visual Studio 2022 17.10): programs built with newer tools can crash with it.
function Test-VcOld {
    param([string]$Version)
    try {
        return ([version]$Version) -lt ([version]'14.40')
    }
    catch {
        return $false
    }
}
# ---- end of shared block vc-runtime ----

$facts = [ordered]@{ x64 = ''; x86 = ''; versions = ''; missing = ''; broken = ''; old = '' }
$versions = New-Object System.Collections.Generic.List[string]
$missing = New-Object System.Collections.Generic.List[string]
$broken = New-Object System.Collections.Generic.List[string]
$old = New-Object System.Collections.Generic.List[string]
foreach ($arch in @(Get-VcArchitecture)) {
    $state = Get-VcRuntime $arch
    $facts[$arch] = $state.version
    if ($state.version.Length -eq 0) {
        $missing.Add($arch)
        continue
    }
    $versions.Add($arch + ' ' + $state.version)
    if (-not $state.files) {
        $broken.Add($arch)
    }
    elseif (Test-VcOld $state.version) {
        $old.Add($arch)
    }
}
$facts.versions = $versions.ToArray() -join ', '
$facts.missing = $missing.ToArray() -join ', '
$facts.broken = $broken.ToArray() -join ', '
$facts.old = $old.ToArray() -join ', '

$result = 'ok'
if ($missing.Count -gt 0) {
    $result = 'missing'
}
elseif ($broken.Count -gt 0) {
    $result = 'broken'
}
elseif ($old.Count -gt 0) {
    $result = 'old'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
