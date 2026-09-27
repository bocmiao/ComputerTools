# Entries of the "New" menu (right-click > New) and where they come from.
# Read-only.
# Explorer builds the New menu from ShellNew keys under the file name
# extensions in the classes: HKLM\SOFTWARE\Classes (all users) and the
# logged-in user's Software\Classes (-UserHive, which wins where both have
# something, as in HKEY_CLASSES_ROOT):
#   <.ext>\ShellNew            the direct form (.txt, .bmp)
#   <.ext>\<ProgID>\ShellNew   the ProgID form (.zip\CompressedFolder,
#                              .docx\Word.Document.12): the one under the
#                              extension's current ProgID counts
# An entry is made by one of the values FileName, Command, Data or NullFile
# (Microsoft, "Extending the New Submenu"); Handler names a COM class that
# makes it. Its text is MenuText, else the type name of the extension's
# current ProgID (ItemName is the name of the new file: "New Text Document").
# Output: result = 'ok', items = one object per ShellNew key: hive (machine /
# user), ext, progid ('' for the direct form), current_progid (the default
# value of the extension, the user's classes first), values (the names of the
# values in the key), item_name, menu_text, type_name (FriendlyTypeName, else
# the default value, of the current ProgID, the user's classes first). Texts
# may be "@file,-id" strings; the engine resolves them.
# The engine decides what is shown, what it switched off (values it renamed)
# and what cannot be switched off. Folder (HKCR\Folder\ShellNew) is not an
# extension and is not listed.

[CmdletBinding()]
param(
    [string]$UserHive = 'HKCU:'
)

$ErrorActionPreference = 'Stop'

$maxItems = 300
$noExpand = [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames

# The classes of all users ('' for -Hive), or of the user in -Hive ('HKCU:' or
# 'Registry::HKEY_USERS\<SID>').
function Open-Classes {
    param([string]$Hive)
    try {
        if ($Hive.Length -eq 0) {
            return [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SOFTWARE\Classes')
        }
        if ($Hive -match '(?i)HKEY_USERS\\(S-1-[0-9-]+)$') {
            return [Microsoft.Win32.Registry]::Users.OpenSubKey($Matches[1] + '\Software\Classes')
        }
        return [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Classes')
    }
    catch {
        return $null
    }
}

$classes = @{ machine = (Open-Classes ''); user = (Open-Classes $UserHive) }

# A string value ('' when it is missing or not a string).
function Get-Text {
    param($Key, [string]$Name)
    $value = $Key.GetValue($Name, $null, $noExpand)
    if ($value -is [string]) {
        return $value.Trim()
    }
    return ''
}

# A value as HKEY_CLASSES_ROOT shows it: the user's classes first.
function Get-Merged {
    param([string]$Path, [string]$Name)
    foreach ($hive in @('user', 'machine')) {
        $root = $classes[$hive]
        if ($null -eq $root) {
            continue
        }
        try {
            $key = $root.OpenSubKey($Path)
        }
        catch {
            continue
        }
        if ($null -eq $key) {
            continue
        }
        try {
            $text = Get-Text $key $Name
        }
        finally {
            $key.Close()
        }
        if ($text.Length -gt 0) {
            return $text
        }
    }
    return ''
}

$items = New-Object System.Collections.Generic.List[object]
foreach ($hive in @('machine', 'user')) {
    $root = $classes[$hive]
    if ($null -eq $root) {
        continue
    }
    foreach ($ext in $root.GetSubKeyNames()) {
        if ((-not $ext.StartsWith('.')) -or ($items.Count -ge $maxItems)) {
            continue
        }
        try {
            $extKey = $root.OpenSubKey($ext)
        }
        catch {
            continue
        }
        if ($null -eq $extKey) {
            continue
        }
        try {
            # '' is the direct form, anything else a ProgID subkey
            $forms = New-Object System.Collections.Generic.List[string]
            foreach ($sub in $extKey.GetSubKeyNames()) {
                if ($sub -eq 'ShellNew') {
                    $forms.Add('')
                    continue
                }
                try {
                    $probe = $extKey.OpenSubKey($sub + '\ShellNew')
                }
                catch {
                    continue
                }
                if ($null -ne $probe) {
                    $probe.Close()
                    $forms.Add($sub)
                }
            }
            if ($forms.Count -eq 0) {
                continue
            }
            $current = Get-Merged $ext ''
            $typeName = ''
            if ($current.Length -gt 0) {
                $typeName = Get-Merged $current 'FriendlyTypeName'
                if ($typeName.Length -eq 0) {
                    $typeName = Get-Merged $current ''
                }
            }
            foreach ($progId in $forms) {
                $path = 'ShellNew'
                if ($progId.Length -gt 0) {
                    $path = $progId + '\ShellNew'
                }
                try {
                    $key = $extKey.OpenSubKey($path)
                }
                catch {
                    continue
                }
                if ($null -eq $key) {
                    continue
                }
                try {
                    $items.Add([ordered]@{
                            hive           = $hive
                            ext            = $ext
                            progid         = $progId
                            current_progid = $current
                            values         = @($key.GetValueNames())
                            item_name      = (Get-Text $key 'ItemName')
                            menu_text      = (Get-Text $key 'MenuText')
                            type_name      = $typeName
                        })
                }
                finally {
                    $key.Close()
                }
            }
        }
        finally {
            $extKey.Close()
        }
    }
}
foreach ($root in $classes.Values) {
    if ($null -ne $root) {
        $root.Close()
    }
}

[pscustomobject]@{
    result = 'ok'
    items  = $items.ToArray()
}
