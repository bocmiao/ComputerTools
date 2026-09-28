# Tool: system.office-repair (action)
# Opens Office's own repair, the same as Apps (Programs and Features) ->
# Office -> Modify (Change): runs the ModifyPath the Office entry registered.
# Click-to-Run Office then shows "How would you like to repair your Office
# programs?" (Quick Repair / Online Repair); MSI Office shows "Change your
# installation" with Repair (Microsoft Support: "Repair an Office
# application"; "Repair process does not start for Office Click-to-Run
# application"). Click-to-Run is repaired first (one repair covers all of its
# products and languages); without it, the first MSI Office.
# Only a program Office put there is started: the file must be msiexec.exe in
# System32 or be under Common Files\Microsoft Shared or Program Files\Microsoft
# Office (64- or 32-bit), exist, and carry a valid signature by Microsoft
# Corporation; the arguments may only hold letters, digits, spaces and
# = . _ - / { }; and the command must open a repair or change screen
# (msiexec /I{product code}, OfficeClickToRun.exe ... scenario=repair,
# setup.exe /modify ...), so an uninstall command is never run.
# The repair window is not waited for. medkit runs elevated and so does the
# repair (it asks for administrator rights anyway).
# Results: started / no-office / no-repair (no Modify command registered) /
# missing (the registered program is gone) / untrusted (not started).
# Facts: name (the Office being repaired), others (other Office entries that
# this repair does not cover).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block office-installs: identical in checks/system/office-installs.ps1 and tools/system/office-repair.ps1 (medkit-data check compares them) ----
# Office entries in Apps (Programs and Features): the subkeys of
# HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall (and, on 64-bit
# Windows, the same key in the 32-bit view) that have a DisplayName, are not
# hidden (SystemComponent is not 1), are published by Microsoft Corporation,
# and are either
# - Click-to-Run: the uninstall command runs OfficeClickToRun.exe. One install
#   can list several products (and one entry per language); its bitness is
#   Platform (x86 / x64) in HKLM\SOFTWARE\Microsoft\Office\ClickToRun\Configuration;
# - or Windows Installer (MSI): the entry is named Office<version>.<product>
#   (Office14 = 2010, Office15 = 2013, Office16 = 2016); its bitness is the
#   registry view it is in.
# ModifyPath is what "Modify" / "Change" in Apps runs: Office's own repair.
# Each entry: Name, Kind (c2r / msi), Version (16 for Click-to-Run), Bits (64,
# 32, or 0 when not known), Modify.
function Get-OfficeInstall {
    $is64 = [Environment]::Is64BitOperatingSystem
    $hive = [Microsoft.Win32.RegistryHive]::LocalMachine
    $views = @([Microsoft.Win32.RegistryView]::Registry64)
    if ($is64) {
        $views += [Microsoft.Win32.RegistryView]::Registry32
    }
    $c2rBits = 0
    $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($hive, [Microsoft.Win32.RegistryView]::Registry64)
    try {
        $config = $base.OpenSubKey('SOFTWARE\Microsoft\Office\ClickToRun\Configuration')
        if ($null -ne $config) {
            $platform = [string]$config.GetValue('Platform')
            $config.Close()
            if ($platform -eq 'x64') {
                $c2rBits = 64
            }
            elseif ($platform -eq 'x86') {
                $c2rBits = 32
            }
        }
    }
    finally {
        $base.Close()
    }
    $found = New-Object System.Collections.Generic.List[object]
    foreach ($view in $views) {
        $bits = 32
        if ($is64 -and ($view -eq [Microsoft.Win32.RegistryView]::Registry64)) {
            $bits = 64
        }
        $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($hive, $view)
        try {
            $uninstall = $base.OpenSubKey('SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall')
            if ($null -eq $uninstall) {
                continue
            }
            try {
                foreach ($keyName in $uninstall.GetSubKeyNames()) {
                    $key = $uninstall.OpenSubKey($keyName)
                    if ($null -eq $key) {
                        continue
                    }
                    try {
                        $name = ([string]$key.GetValue('DisplayName')).Trim()
                        $hidden = [string]$key.GetValue('SystemComponent')
                        $publisher = ([string]$key.GetValue('Publisher')).Trim()
                        $remove = [string]$key.GetValue('UninstallString')
                        $modify = [string]$key.GetValue('ModifyPath')
                    }
                    finally {
                        $key.Close()
                    }
                    if (($name.Length -eq 0) -or ($hidden -eq '1') -or ($publisher -ne 'Microsoft Corporation')) {
                        continue
                    }
                    if ($remove -match 'OfficeClickToRun\.exe') {
                        $found.Add([pscustomobject]@{ Name = $name; Kind = 'c2r'; Version = 16; Bits = $c2rBits; Modify = $modify })
                    }
                    elseif ($keyName -match '^Office(\d{2})\.[A-Za-z0-9]+$') {
                        $found.Add([pscustomobject]@{ Name = $name; Kind = 'msi'; Version = [int]$Matches[1]; Bits = $bits; Modify = $modify })
                    }
                }
            }
            finally {
                $uninstall.Close()
            }
        }
        finally {
            $base.Close()
        }
    }
    return $found.ToArray()
}
# ---- end of shared block office-installs ----

# The program and the arguments of a registered command line: a quoted program
# first, else everything up to the first ".exe". $null when there is neither.
function Split-CommandLine {
    param([string]$Command)
    $text = $Command.Trim()
    if ($text.StartsWith('"')) {
        $close = $text.IndexOf('"', 1)
        if ($close -lt 2) {
            return $null
        }
        return [pscustomobject]@{ File = $text.Substring(1, $close - 1); Arguments = $text.Substring($close + 1).Trim() }
    }
    $exe = $text.IndexOf('.exe', [System.StringComparison]::OrdinalIgnoreCase)
    if ($exe -lt 1) {
        return $null
    }
    return [pscustomobject]@{ File = $text.Substring(0, $exe + 4); Arguments = $text.Substring($exe + 4).Trim() }
}

# '' when $Path carries a valid Authenticode signature by Microsoft Corporation;
# else why not: the signature status (NotSigned, HashMismatch, UnknownError...),
# NotMicrosoft, or the error
function Get-SignatureProblem {
    param([string]$Path)
    try {
        $signature = Get-AuthenticodeSignature -FilePath $Path
    }
    catch {
        return ('Error ' + $_.Exception.GetType().Name)
    }
    if ([string]$signature.Status -ne 'Valid') {
        return [string]$signature.Status
    }
    if (([string]$signature.SignerCertificate.Subject) -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
        return 'NotMicrosoft'
    }
    return ''
}

# Folders a program Office registered for "Modify" may be in.
function Get-OfficeProgramFolder {
    $folders = New-Object System.Collections.Generic.List[string]
    foreach ($pair in @(@('CommonProgramFiles', 'Microsoft Shared'), @('CommonProgramFilesX86', 'Microsoft Shared'), @('ProgramFiles', 'Microsoft Office'), @('ProgramFilesX86', 'Microsoft Office'))) {
        $root = [Environment]::GetFolderPath($pair[0])
        if (-not [string]::IsNullOrWhiteSpace($root)) {
            $folders.Add(([System.IO.Path]::GetFullPath((Join-Path $root $pair[1]))).TrimEnd('\') + '\')
        }
    }
    return $folders.ToArray()
}

function New-Result {
    param([string]$Code, [string]$Name, [int]$Others)
    return [pscustomobject]@{
        result = $Code
        facts  = [ordered]@{
            name   = $Name
            others = $Others
        }
    }
}

$installs = @(Get-OfficeInstall)
$ordered = @($installs | Where-Object { $_.Kind -eq 'c2r' }) + @($installs | Where-Object { $_.Kind -eq 'msi' })
if ($ordered.Count -eq 0) {
    New-Result -Code 'no-office' -Name '' -Others 0
    return
}
$target = $ordered[0]
# One Click-to-Run repair covers all of its entries; every MSI Office is its own
$others = @($installs | Where-Object { ($_.Kind -ne $target.Kind) -or (($_.Kind -eq 'msi') -and ($_.Name -ne $target.Name)) }).Count

if ([string]::IsNullOrWhiteSpace($target.Modify)) {
    New-Result -Code 'no-repair' -Name $target.Name -Others $others
    return
}
$parts = Split-CommandLine -Command $target.Modify
if ($null -eq $parts) {
    New-Result -Code 'untrusted' -Name $target.Name -Others $others
    return
}
$file = [Environment]::ExpandEnvironmentVariables($parts.File)
$msiexec = Join-Path ([Environment]::SystemDirectory) 'msiexec.exe'
if (-not [System.IO.Path]::IsPathRooted($file)) {
    # MSI entries may register a bare "MsiExec.exe /I{product code}"
    if ([System.IO.Path]::GetFileName($file) -ne 'msiexec.exe') {
        New-Result -Code 'untrusted' -Name $target.Name -Others $others
        return
    }
    $file = $msiexec
}
$file = [System.IO.Path]::GetFullPath($file)
$allowed = ($file -eq $msiexec)
foreach ($folder in @(Get-OfficeProgramFolder)) {
    if ($file.StartsWith($folder, [System.StringComparison]::OrdinalIgnoreCase)) {
        $allowed = $true
    }
}
# Only the repair / change screens, never an uninstall command
$leaf = [System.IO.Path]::GetFileName($file)
$repairs = $false
if ($leaf -eq 'msiexec.exe') {
    $repairs = $parts.Arguments -match '^/I\{[0-9A-Fa-f-]+\}$'
}
elseif ($leaf -eq 'OfficeClickToRun.exe') {
    $repairs = $parts.Arguments -match '(^|\s)scenario=repair(\s|$)'
}
elseif ($leaf -eq 'setup.exe') {
    $repairs = $parts.Arguments -match '^/modify(\s|$)'
}
if ((-not $allowed) -or (-not $repairs) -or ($parts.Arguments -notmatch '^[A-Za-z0-9 =._/{}\-]*$')) {
    New-Result -Code 'untrusted' -Name $target.Name -Others $others
    return
}
if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
    New-Result -Code 'missing' -Name $target.Name -Others $others
    return
}
$problem = Get-SignatureProblem -Path $file
if ($problem.Length -gt 0) {
    Write-Verbose ('Not started, signature: {0}' -f $problem)
    New-Result -Code 'untrusted' -Name $target.Name -Others $others
    return
}

if ($parts.Arguments.Length -gt 0) {
    Start-Process -FilePath $file -ArgumentList $parts.Arguments
}
else {
    Start-Process -FilePath $file
}
New-Result -Code 'started' -Name $target.Name -Others $others
