# Check: system.photo-viewer
# Can Windows Photo Viewer open common pictures here? The verify check of the
# feature explorer.photo-viewer; not part of the health check.
# Windows Photo Viewer (PhotoViewer.dll in %ProgramFiles%\Windows Photo Viewer,
# run through rundll32) still ships with Windows 10 and 11, but only TIFF is
# registered for it: PhotoViewer.FileAssoc.Tiff in Capabilities\
# FileAssociations of HKLM\SOFTWARE\Microsoft\Windows Photo Viewer. The
# feature registers JPEG, PNG, BMP and GIF the same way, each with its own
# ProgID.
#   missing         PhotoViewer.dll is not there (stripped-down builds)
#   registered      each extension below points at its ProgID in
#                   FileAssociations, is listed under its OpenWithProgids,
#                   and the ProgID has an open command
#   not-registered  otherwise
# Read-only. Facts: unregistered (the extensions that are not registered).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$expected = [ordered]@{
    '.jpg'  = 'PhotoViewer.FileAssoc.Jpeg'
    '.jpeg' = 'PhotoViewer.FileAssoc.Jpeg'
    '.jpe'  = 'PhotoViewer.FileAssoc.Jpeg'
    '.jfif' = 'PhotoViewer.FileAssoc.Jpeg'
    '.png'  = 'PhotoViewer.FileAssoc.Png'
    '.bmp'  = 'PhotoViewer.FileAssoc.Bitmap'
    '.dib'  = 'PhotoViewer.FileAssoc.Bitmap'
    '.gif'  = 'PhotoViewer.FileAssoc.Gif'
}

# The 64-bit Program Files also from 32-bit PowerShell (Explorer expands
# %ProgramFiles% of the open command as a 64-bit process).
$programFiles = $env:ProgramW6432
if ([string]::IsNullOrEmpty($programFiles)) {
    $programFiles = $env:ProgramFiles
}
$dll = Join-Path $programFiles 'Windows Photo Viewer\PhotoViewer.dll'
if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) {
    return [pscustomobject]@{ result = 'missing'; facts = [ordered]@{ unregistered = '' } }
}

$machine = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry64)

function Get-RegText {
    param([string]$Path, [string]$Name)
    $key = $machine.OpenSubKey($Path)
    if ($null -eq $key) {
        return $null
    }
    try {
        $value = $key.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        if ($null -eq $value) {
            return $null
        }
        return [string]$value
    }
    finally {
        $key.Close()
    }
}

$unregistered = New-Object System.Collections.Generic.List[string]
foreach ($ext in $expected.Keys) {
    $progId = $expected[$ext]
    $assoc = Get-RegText 'SOFTWARE\Microsoft\Windows Photo Viewer\Capabilities\FileAssociations' $ext
    $openWith = Get-RegText ('SOFTWARE\Classes\' + $ext + '\OpenWithProgids') $progId
    $command = Get-RegText ('SOFTWARE\Classes\' + $progId + '\shell\open\command') ''
    if (($assoc -ne $progId) -or ($null -eq $openWith) -or ([string]::IsNullOrEmpty($command))) {
        $unregistered.Add($ext)
    }
}
$machine.Close()

$result = 'registered'
if ($unregistered.Count -gt 0) {
    $result = 'not-registered'
}
[pscustomobject]@{
    result = $result
    facts  = [ordered]@{ unregistered = ($unregistered.ToArray() -join ', ') }
}
