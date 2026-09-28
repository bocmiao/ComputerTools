# Check: system.office-installs
# Which Office products are installed, the way Windows lists them in Apps
# (Programs and Features), and whether they break Microsoft's rules for more
# than one Office on a PC (Microsoft Support: "Office installed with
# Click-to-Run and Windows Installer on same computer isn't supported" and
# "Install and use different versions of Office on the same PC"):
# - the same version installed with both technologies: Click-to-Run Office
#   (Microsoft 365, and Office 2016 to 2024 bought as a product: all version
#   16) next to Windows Installer (MSI) Office 2016 (Office16.*)
#   -> same-version;
# - 32-bit and 64-bit Office on the same PC -> mixed-bitness.
# Read-only. Nothing is uninstalled here: the symptom points to Microsoft's
# own uninstall support tool.
# Result codes: none / ok / same-version / mixed-bitness.
# Facts: products (each name with its bitness, joined by the Chinese list
# comma), count.

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

$installs = @(Get-OfficeInstall)

# "Name (64 bit)" in Chinese: full-width brackets, U+4F4D after the number
$names = New-Object System.Collections.Generic.List[string]
foreach ($office in $installs) {
    $label = $office.Name
    if ($office.Bits -gt 0) {
        $label = $label + [char]0xFF08 + [string]$office.Bits + ' ' + [char]0x4F4D + [char]0xFF09
    }
    $names.Add($label)
}

$c2r = @($installs | Where-Object { $_.Kind -eq 'c2r' })
$msi2016 = @($installs | Where-Object { ($_.Kind -eq 'msi') -and ($_.Version -eq 16) })
$bitness = @($installs | Where-Object { $_.Bits -gt 0 } | ForEach-Object { $_.Bits } | Sort-Object -Unique)
$result = 'ok'
if ($installs.Count -eq 0) {
    $result = 'none'
}
elseif (($c2r.Count -gt 0) -and ($msi2016.Count -gt 0)) {
    $result = 'same-version'
}
elseif ($bitness.Count -gt 1) {
    $result = 'mixed-bitness'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        products = ($names -join [string][char]0x3001)
        count    = $installs.Count
    }
}
