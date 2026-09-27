# Feature: network.hosts-cleanup -- detect
# applied: no line of the hosts file holds a flagged name (shared block
# hosts-rules); not-applied: some line does. The engine decides with the
# verify check network.hosts; this script is what undo checks first.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block hosts-rules: identical in checks/network/hosts.ps1 and features/network/hosts-cleanup-*.ps1 (medkit-data check compares them) ----
# The hosts file maps host names to addresses before DNS is asked. Its folder
# is DataBasePath under HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters
# (default %SystemRoot%\System32\drivers\etc); malware sometimes points it
# elsewhere. A line is "address name [name ...]"; '#' starts a comment.
# A name is flagged when it is, or is under, a domain of one of the groups
# below (microsoft.com itself only as exact names: blocking its telemetry
# subdomains is a deliberate privacy choice, not a fault):
#   security   antivirus and security sites (blocking them is a classic
#              malware trick: the antivirus cannot update)
#   microsoft  Windows Update, activation, the connectivity test (blocked:
#              the "no Internet" icon), Defender and SmartScreen, sign-in
#              (the endpoints Microsoft lists in "Connection endpoints for
#              Windows 11"; sls.microsoft.com is the activation server)
#   common     sites most people use (search, chat, shopping, mail, video)
# It "blocks" when the address is 0.0.0.0, 127.x.x.x, :: or ::1, and
# "redirects" for any other address (a site pinned to a fixed address: a
# hijack, or an old "speed-up" guide whose address has since changed).
# localhost is never flagged.
# The file is read and written as bytes: Latin-1 maps every byte to one
# character and back, so comments in any code page survive unchanged (UTF-16
# files, with a byte order mark, are read as UTF-16). Lines are split at LF
# and keep their CR, so line endings survive too.
$hostsGroups = @(
    [pscustomobject]@{
        Id     = 'security'
        Exact  = @()
        Suffix = @('360.cn', '360.com', 'huorong.cn', 'kaspersky.com', 'kaspersky.com.cn', 'eset.com', 'avast.com', 'avg.com',
            'avira.com', 'bitdefender.com', 'mcafee.com', 'norton.com', 'malwarebytes.com', 'trendmicro.com', 'virustotal.com',
            'rising.com.cn', 'duba.com')
    },
    [pscustomobject]@{
        Id     = 'microsoft'
        Exact  = @('microsoft.com', 'www.microsoft.com')
        Suffix = @('windowsupdate.com', 'update.microsoft.com', 'windowsupdate.microsoft.com', 'delivery.mp.microsoft.com',
            'dsp.mp.microsoft.com', 'msftconnecttest.com', 'msftncsi.com', 'sls.microsoft.com', 'licensing.mp.microsoft.com',
            'definitionupdates.microsoft.com', 'wdcp.microsoft.com', 'smartscreen.microsoft.com', 'smartscreen-prod.microsoft.com',
            'checkappexec.microsoft.com', 'login.live.com')
    },
    [pscustomobject]@{
        Id     = 'common'
        Exact  = @()
        Suffix = @('baidu.com', 'qq.com', 'taobao.com', 'tmall.com', 'jd.com', 'alipay.com', '163.com', '126.com', 'sina.com.cn',
            'weibo.com', 'bilibili.com', 'douyin.com', 'zhihu.com', 'bing.com', 'google.com', 'apple.com', 'icloud.com', 'github.com')
    }
)

# Name -> group, looked up for the name and each parent domain (a hosts file
# that blocks ads can have tens of thousands of lines).
$hostsExact = @{}
$hostsSuffix = @{}
foreach ($group in $hostsGroups) {
    foreach ($domain in $group.Exact) {
        $hostsExact[$domain] = $group.Id
    }
    foreach ($domain in $group.Suffix) {
        $hostsSuffix[$domain] = $group.Id
    }
}

function Get-HostsGroup {
    param([string]$Name)
    if ($hostsExact.ContainsKey($Name)) {
        return $hostsExact[$Name]
    }
    $candidate = $Name
    while ($candidate.Length -gt 0) {
        if ($hostsSuffix.ContainsKey($candidate)) {
            return $hostsSuffix[$candidate]
        }
        $dot = $candidate.IndexOf('.')
        if ($dot -lt 0) {
            break
        }
        $candidate = $candidate.Substring($dot + 1)
    }
    return ''
}

function Test-BlockingAddress {
    param([string]$Address)
    return ($Address -match '^(0\.0\.0\.0|127\.\d{1,3}\.\d{1,3}\.\d{1,3}|::|::1|0:0:0:0:0:0:0:0|0:0:0:0:0:0:0:1)$')
}

# Where Windows reads the hosts file from, and whether that was changed.
function Get-HostsLocation {
    $default = [Environment]::ExpandEnvironmentVariables('%SystemRoot%\System32\drivers\etc').TrimEnd('\')
    $folder = $default
    $changed = $false
    $raw = $null
    try {
        $raw = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters' -Name 'DataBasePath' -ErrorAction Stop).DataBasePath
    }
    catch {
        $raw = $null
    }
    if ($null -ne $raw) {
        $value = [Environment]::ExpandEnvironmentVariables(([string]$raw).Trim()).TrimEnd('\')
        if ($value.Length -gt 0) {
            $folder = $value
            $changed = -not [string]::Equals($value, $default, [StringComparison]::OrdinalIgnoreCase)
        }
    }
    return [pscustomobject]@{ Path = (Join-Path $folder 'hosts'); Changed = $changed }
}

# The file as text that round-trips to the same bytes: its encoding, its byte
# order mark (kept apart, so that removing the first line cannot drop it) and
# the text after it.
function Read-HostsText {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [pscustomobject]@{ Exists = $false; Text = ''; Encoding = 'latin1'; Bom = '' }
    }
    # Another program (an antivirus, the DNS client) can have the file open
    # for a moment: tried again for about a second before giving up.
    $bytes = $null
    for ($attempt = 1; $null -eq $bytes; $attempt++) {
        try {
            $bytes = [IO.File]::ReadAllBytes($Path)
        }
        catch {
            if (($attempt -ge 10) -or -not (Test-HostsBusy $_.Exception)) {
                throw
            }
            Start-Sleep -Milliseconds 100
        }
    }
    $encoding = 'latin1'
    if (($bytes.Length -ge 2) -and ($bytes[0] -eq 0xFF) -and ($bytes[1] -eq 0xFE)) {
        $encoding = 'utf16le'
    }
    elseif (($bytes.Length -ge 2) -and ($bytes[0] -eq 0xFE) -and ($bytes[1] -eq 0xFF)) {
        $encoding = 'utf16be'
    }
    $text = (Get-HostsEncoding $encoding).GetString($bytes)
    $bom = ''
    if ($encoding -ne 'latin1') {
        $bom = [string][char]0xFEFF
    }
    elseif (($bytes.Length -ge 3) -and ($bytes[0] -eq 0xEF) -and ($bytes[1] -eq 0xBB) -and ($bytes[2] -eq 0xBF)) {
        $bom = $text.Substring(0, 3)
    }
    if (($bom.Length -gt 0) -and $text.StartsWith($bom)) {
        $text = $text.Substring($bom.Length)
    }
    return [pscustomobject]@{ Exists = $true; Text = $text; Encoding = $encoding; Bom = $bom }
}

function Get-HostsEncoding {
    param([string]$Name)
    switch ($Name) {
        'utf16le' { return [Text.Encoding]::Unicode }
        'utf16be' { return [Text.Encoding]::BigEndianUnicode }
        default { return [Text.Encoding]::GetEncoding(28591) }
    }
}

# Whether an error is the file being open in another program: a sharing or
# lock violation (Windows errors 32 and 33).
function Test-HostsBusy {
    param($Exception)
    $code = $Exception.GetBaseException().HResult
    return (($code -eq -2147024864) -or ($code -eq -2147024863))
}

# Writes the bytes, tried again for about a second while another program has
# the file open (the DNS client reads it right after every change).
function Write-HostsBytes {
    param([string]$Path, [byte[]]$Bytes)
    for ($attempt = 1; ; $attempt++) {
        try {
            [IO.File]::WriteAllBytes($Path, $Bytes)
            return
        }
        catch {
            if (($attempt -ge 10) -or -not (Test-HostsBusy $_.Exception)) {
                throw
            }
            Start-Sleep -Milliseconds 100
        }
    }
}

# Writes the byte order mark and the text back with the same encoding. A
# read-only file is made writable for the write and read-only again after.
function Write-HostsText {
    param([string]$Path, [string]$Text, [string]$Encoding, [string]$Bom)
    $bytes = (Get-HostsEncoding $Encoding).GetBytes($Bom + $Text)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Write-HostsBytes $Path $bytes
        return
    }
    $item = Get-Item -LiteralPath $Path -Force
    $attributes = $item.Attributes
    $readOnly = ([int]$attributes -band [int][IO.FileAttributes]::ReadOnly) -ne 0
    if ($readOnly) {
        $item.Attributes = [IO.FileAttributes]([int]$attributes -band (-bnot [int][IO.FileAttributes]::ReadOnly))
    }
    try {
        Write-HostsBytes $Path $bytes
    }
    finally {
        if ($readOnly) {
            (Get-Item -LiteralPath $Path -Force).Attributes = $attributes
        }
    }
}

# The names on active lines: Count (all of them, localhost not counted) and
# Flagged (each flagged name with its line index, group and kind: blocks /
# redirects).
function Get-HostsEntries {
    param([string[]]$Lines)
    $count = 0
    $flagged = New-Object System.Collections.Generic.List[object]
    for ($i = 0; $i -lt $Lines.Count; $i++) {
        $text = $Lines[$i]
        $hash = $text.IndexOf('#')
        if ($hash -ge 0) {
            $text = $text.Substring(0, $hash)
        }
        $text = $text.Trim()
        if ($text.Length -eq 0) {
            continue
        }
        $parts = $text -split '\s+'
        if ($parts.Count -lt 2) {
            continue
        }
        $blocking = Test-BlockingAddress $parts[0]
        for ($j = 1; $j -lt $parts.Count; $j++) {
            $name = $parts[$j].ToLowerInvariant().TrimEnd('.')
            if ($name -eq 'localhost') {
                continue
            }
            $count++
            $group = Get-HostsGroup $name
            if ($group.Length -eq 0) {
                continue
            }
            $kind = 'redirects'
            if ($blocking) {
                $kind = 'blocks'
            }
            $flagged.Add([pscustomobject]@{ Line = $i; Name = $name; Group = $group; Kind = $kind })
        }
    }
    return [pscustomobject]@{ Count = $count; Flagged = $flagged.ToArray() }
}
# ---- end of shared block hosts-rules ----

$location = Get-HostsLocation
$file = Read-HostsText $location.Path
$flagged = @((Get-HostsEntries @($file.Text.Split([char]10))).Flagged)
$state = 'applied'
if ($flagged.Count -gt 0) {
    $state = 'not-applied'
}
[pscustomobject]@{ state = $state; facts = [ordered]@{ flagged = $flagged.Count } }
