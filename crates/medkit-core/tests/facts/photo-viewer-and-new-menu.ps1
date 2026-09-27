# Read-only facts for two planned features, printed by the Windows tests:
#   - "restore Windows Photo Viewer": whether PhotoViewer.dll and its ProgIDs
#     are here, what they and the built-in image ProgIDs contain (names,
#     icons, commands), and the strings PhotoViewer.dll carries
#   - "manage the New menu": every ShellNew entry (HKCR\.ext\ShellNew or
#     HKCR\.ext\<ProgID>\ShellNew) in the machine and user classes, and
#     Explorer's cache of them
# Changes nothing.

$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

function Format-Value {
    param($Value)
    if ($Value -is [byte[]]) {
        return (($Value | Select-Object -First 64 | ForEach-Object { $_.ToString('x2') }) -join ',')
    }
    if ($Value -is [string[]]) {
        return ($Value -join ' | ')
    }
    return [string]$Value
}

function Write-Key {
    param([Microsoft.Win32.RegistryKey]$Key, [int]$Depth)
    '  [{0}]' -f $Key.Name
    foreach ($name in $Key.GetValueNames()) {
        $kind = $Key.GetValueKind($name)
        $value = $Key.GetValue($name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $label = $name
        if ($name.Length -eq 0) {
            $label = '@'
        }
        '    {0} ({1}) = {2}' -f $label, $kind, (Format-Value $value)
    }
    if ($Depth -gt 0) {
        foreach ($sub in $Key.GetSubKeyNames()) {
            $child = $Key.OpenSubKey($sub)
            if ($null -ne $child) {
                Write-Key $child ($Depth - 1)
                $child.Close()
            }
        }
    }
}

function Write-Path {
    param([Microsoft.Win32.RegistryKey]$Hive, [string]$Path, [int]$Depth = 6)
    $key = $Hive.OpenSubKey($Path)
    if ($null -eq $key) {
        '  (missing) {0}\{1}' -f $Hive.Name, $Path
        return
    }
    Write-Key $key $Depth
    $key.Close()
}

$hklm = [Microsoft.Win32.Registry]::LocalMachine
$hkcu = [Microsoft.Win32.Registry]::CurrentUser

'== PhotoViewer.dll'
$dll = Join-Path $env:ProgramFiles 'Windows Photo Viewer\PhotoViewer.dll'
'  {0}: {1}' -f $dll, (Test-Path -LiteralPath $dll -PathType Leaf)
$dll86 = Join-Path ${env:ProgramFiles(x86)} 'Windows Photo Viewer\PhotoViewer.dll'
'  {0}: {1}' -f $dll86, (Test-Path -LiteralPath $dll86 -PathType Leaf)

'== ProgIDs'
foreach ($progId in @('PhotoViewer.FileAssoc.Tiff', 'PhotoViewer.FileAssoc.Jpeg', 'PhotoViewer.FileAssoc.JFIF', 'PhotoViewer.FileAssoc.Png',
        'PhotoViewer.FileAssoc.Bitmap', 'PhotoViewer.FileAssoc.Gif', 'PhotoViewer.FileAssoc.Wdp', 'jpegfile', 'pjpegfile', 'pngfile', 'giffile',
        'Paint.Picture', 'TIFImage.Document', 'icofile', 'Applications\photoviewer.dll')) {
    '-- ' + $progId
    Write-Path $hklm ('SOFTWARE\Classes\' + $progId)
}

'== Capabilities'
Write-Path $hklm 'SOFTWARE\Microsoft\Windows Photo Viewer'
'== RegisteredApplications'
$registered = $hklm.OpenSubKey('SOFTWARE\RegisteredApplications')
if ($null -ne $registered) {
    '  Windows Photo Viewer = {0}' -f $registered.GetValue('Windows Photo Viewer')
}

'== Extensions'
foreach ($ext in @('.jpg', '.jpeg', '.jpe', '.jfif', '.png', '.bmp', '.dib', '.gif', '.tif', '.tiff', '.ico', '.wdp', '.jxr', '.webp', '.heic')) {
    '-- ' + $ext
    Write-Path $hklm ('SOFTWARE\Classes\' + $ext) 2
}

'== PhotoViewer.dll strings'
Add-Type -Namespace MedkitFacts -Name Shell -MemberDefinition @'
[DllImport("shlwapi.dll", CharSet = CharSet.Unicode)]
public static extern int SHLoadIndirectString(string source, System.Text.StringBuilder output, int size, System.IntPtr reserved);
'@
foreach ($id in 3040..3075) {
    $text = New-Object System.Text.StringBuilder 512
    $source = '@%ProgramFiles%\Windows Photo Viewer\PhotoViewer.dll,-' + $id
    if ([MedkitFacts.Shell]::SHLoadIndirectString($source, $text, 512, [IntPtr]::Zero) -eq 0) {
        '  -{0} = {1}' -f $id, $text.ToString()
    }
}

'== ShellNew'
foreach ($hive in @($hklm, $hkcu)) {
    $classesPath = 'SOFTWARE\Classes'
    $classes = $hive.OpenSubKey($classesPath)
    if ($null -eq $classes) {
        continue
    }
    foreach ($name in $classes.GetSubKeyNames()) {
        if (-not ($name.StartsWith('.') -or ($name -eq 'Folder'))) {
            continue
        }
        $extKey = $classes.OpenSubKey($name)
        if ($null -eq $extKey) {
            continue
        }
        $progId = [string]$extKey.GetValue('')
        foreach ($path in @('ShellNew', ($progId + '\ShellNew'))) {
            if (($path -eq '\ShellNew') -or ($null -eq $extKey.OpenSubKey($path))) {
                continue
            }
            '-- {0}\{1}\{2} (default ProgID {3})' -f $hive.Name, $name, $path, $progId
            Write-Path $classes ($name + '\' + $path) 1
        }
        # ShellNew under a ProgID subkey that is not the default one
        foreach ($sub in $extKey.GetSubKeyNames()) {
            if (($sub -ne $progId) -and ($null -ne $extKey.OpenSubKey($sub + '\ShellNew'))) {
                '-- (not default) {0}\{1}\{2}\ShellNew' -f $hive.Name, $name, $sub
            }
        }
        $extKey.Close()
    }
    $classes.Close()
}

'== Explorer cache of the New menu'
Write-Path $hkcu 'Software\Microsoft\Windows\CurrentVersion\Explorer\Discardable\PostSetup\ShellNew'
