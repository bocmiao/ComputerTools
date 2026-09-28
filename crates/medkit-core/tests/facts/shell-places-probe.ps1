# What the Windows shell itself shows, for the Windows test of the File
# Explorer icons manager (shell_places). Changes nothing. A new process each
# time, so nothing is cached from an earlier look.
#   pc        names in This PC as Explorer lists them by default (folders and
#             other items, hidden ones left out: SHCONTF_FOLDERS |
#             SHCONTF_NONFOLDERS)
#   pc_all    the same with hidden ones (SHCONTF_INCLUDEHIDDEN)
#   pinned    System.IsPinnedToNameSpaceTree of each -Nav item (CLSIDs,
#             comma separated) as the shell reads it (the navigation pane
#             shows root items where it is on)
#   merged    the default value of HKEY_CLASSES_ROOT\CLSID\<-Nav item> (does a
#             value only in the machine's key show through when the user has
#             a key of their own?)
# Output: one line of JSON.

[CmdletBinding()]
param(
    [string]$Nav = ''
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

$shell = New-Object -ComObject Shell.Application

function Get-PcNames {
    param([int]$Flags)
    $folder = $shell.NameSpace(17)
    if ($null -eq $folder) {
        return @()
    }
    $items = $folder.Items()
    $items.Filter($Flags, '*')
    return @($items | ForEach-Object { [string]$_.Name })
}

$pinned = [ordered]@{}
$merged = [ordered]@{}
foreach ($clsid in @($Nav -split ',' | Where-Object { $_.Length -gt 0 })) {
    $value = $null
    try {
        $folder = $shell.NameSpace('shell:::' + $clsid)
        if ($null -ne $folder) {
            $value = $folder.Self.ExtendedProperty('System.IsPinnedToNameSpaceTree')
        }
    }
    catch {
        $value = 'error: ' + $_.Exception.Message
    }
    $pinned[$clsid] = $value
    $text = $null
    $key = [Microsoft.Win32.Registry]::ClassesRoot.OpenSubKey('CLSID\' + $clsid)
    if ($null -ne $key) {
        $text = $key.GetValue('')
        $key.Close()
    }
    $merged[$clsid] = $text
}

[pscustomobject]@{
    pc     = @(Get-PcNames 0x60)
    pc_all = @(Get-PcNames 0xE0)
    pinned = $pinned
    merged = $merged
} | ConvertTo-Json -Compress -Depth 4
