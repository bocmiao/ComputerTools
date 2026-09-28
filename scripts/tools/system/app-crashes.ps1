# Tool: system.app-crashes (info)
# Programs and games that crashed or stopped responding in the last $days
# days, from the Application log, grouped by program. Read-only.
#   Application Error 1000   a crash: the values are, in this order, the
#                            program's file name, its version and time stamp,
#                            the faulting module's file name, version and time
#                            stamp, the exception code ("c0000005", sometimes
#                            "0xc0000005"), then the offset, the process, and
#                            the paths (not used: they can hold the user
#                            name), report ID and package.
#   Application Hang 1002    stopped responding: the first value is the
#                            program's file name.
# Per program (at most $maxApps, the most crashes first): how many crashes and
# hangs, the last time, the faulting module and exception code seen most
# often, and what that most likely points to ($causes, from the module's file
# name, then the exception code):
#   graphics   the graphics driver or Direct3D / OpenGL / Vulkan
#   directx    an old DirectX component (d3dx9_43.dll, xinput1_3.dll ...)
#   vc         the Microsoft Visual C++ runtime
#   dotnet     .NET (the runtime's modules, or exception e0434352)
#   overlay    an overlay or recording tool hooked into the game (Steam,
#              Discord, RivaTuner, OBS, the Nahimic audio software ...)
#   ime        an input method (Sogou, QQ Pinyin, Baidu ...)
#   anticheat  a game's anti-cheat protection
#   cpu        c000001d: an instruction the processor does not have (AVX...)
#   missing    c0000135 / c000007b: a DLL is missing or of the wrong kind
#   self       the program's own code or a general Windows module
#              (ntdll.dll, KERNELBASE.dll, ...): the program itself
# Only file names are reported, never paths.
# Result codes: found / none. Facts: days, apps, crashes, hangs.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$days = 14
$maxApps = 8
$maxEvents = 2000

# Module file name patterns (lower case, -like) and what they point to; the
# first match wins.
$causes = @(
    @('nvwgf2um*.dll', 'graphics'), @('nvd3dum*.dll', 'graphics'), @('nvoglv*.dll', 'graphics'),
    @('nvldumd*.dll', 'graphics'), @('nvcuda*.dll', 'graphics'), @('nvgpucomp*.dll', 'graphics'),
    @('atidxx*.dll', 'graphics'), @('atiumd*.dll', 'graphics'), @('atio6axx.dll', 'graphics'),
    @('amdxx*.dll', 'graphics'), @('amdxc*.dll', 'graphics'), @('amdvlk*.dll', 'graphics'), @('aticfx*.dll', 'graphics'),
    @('igd10iumd*.dll', 'graphics'), @('igd12umd*.dll', 'graphics'), @('igdumdim*.dll', 'graphics'),
    @('igxelpicd*.dll', 'graphics'), @('igc*.dll', 'graphics'), @('ig*icd*.dll', 'graphics'), @('igvk*.dll', 'graphics'),
    @('d3d9.dll', 'graphics'), @('d3d10*.dll', 'graphics'), @('d3d11.dll', 'graphics'), @('d3d12*.dll', 'graphics'),
    @('dxgi.dll', 'graphics'), @('opengl32.dll', 'graphics'), @('vulkan-1.dll', 'graphics'),
    @('d3dx9_*.dll', 'directx'), @('d3dx10_*.dll', 'directx'), @('d3dx11_*.dll', 'directx'),
    @('xinput1_*.dll', 'directx'), @('xaudio2_*.dll', 'directx'), @('x3daudio*.dll', 'directx'),
    @('msvcp*.dll', 'vc'), @('vcruntime*.dll', 'vc'), @('msvcr*.dll', 'vc'), @('ucrtbase.dll', 'vc'),
    @('concrt*.dll', 'vc'), @('mfc*.dll', 'vc'), @('vcomp*.dll', 'vc'),
    @('clr.dll', 'dotnet'), @('coreclr.dll', 'dotnet'), @('clrjit.dll', 'dotnet'), @('mscorwks.dll', 'dotnet'),
    @('mscorlib*.dll', 'dotnet'), @('system.*.dll', 'dotnet'),
    @('nahimic*.dll', 'overlay'), @('rtsshooks*.dll', 'overlay'), @('gameoverlayrenderer*.dll', 'overlay'),
    @('discordhook*.dll', 'overlay'), @('graphics-hook*.dll', 'overlay'), @('obs-*.dll', 'overlay'),
    @('igo*.dll', 'overlay'), @('nvspcap*.dll', 'overlay'), @('overlay*.dll', 'overlay'),
    @('*.ime', 'ime'), @('sogou*.dll', 'ime'), @('qqpinyin*.dll', 'ime'), @('baidupinyin*.dll', 'ime'),
    @('easyanticheat*.dll', 'anticheat'), @('beclient*.dll', 'anticheat'), @('xigncode*.dll', 'anticheat'),
    @('vgc*.dll', 'anticheat')
)

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

function Get-AppEvent {
    param([hashtable]$Filter)
    try {
        return @(Get-WinEvent -FilterHashtable $Filter -MaxEvents $maxEvents -ErrorAction Stop)
    }
    catch {
        # Get-WinEvent reports "no events" as an error; that simply means none.
        if ([string]$_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {
            return @()
        }
        throw
    }
}

# The n-th value of an event, as text ('' when it is not there).
function Get-Value {
    param($Record, [int]$Index)
    $properties = @($Record.Properties)
    if ($Index -ge $properties.Count) {
        return ''
    }
    return Get-Text $properties[$Index].Value
}

# A file name only: no path, at most 60 characters.
function Get-FileName {
    param([string]$Text)
    $name = (($Text -split '\\')[-1]).Trim()
    if ($name.Length -gt 60) {
        $name = $name.Substring(0, 60)
    }
    return $name
}

# What a faulting module and exception code most likely point to.
function Get-Cause {
    param([string]$Module, [string]$Code)
    $m = $Module.ToLowerInvariant()
    foreach ($pair in $causes) {
        if ($m -like $pair[0]) {
            return $pair[1]
        }
    }
    switch ($Code) {
        'e0434352' {
            return 'dotnet'
        }
        'c000001d' {
            return 'cpu'
        }
        'c0000135' {
            return 'missing'
        }
        'c000007b' {
            return 'missing'
        }
    }
    return 'self'
}

$since = (Get-Date).AddDays(-$days)
$apps = @{}

function Get-AppEntry {
    param([string]$Name)
    $key = $Name.ToLowerInvariant()
    if (-not $apps.ContainsKey($key)) {
        $apps[$key] = [pscustomobject]@{
            Name    = $Name
            Crashes = 0
            Hangs   = 0
            Last    = [datetime]::MinValue
            Faults  = @{}
        }
    }
    return $apps[$key]
}

$crashCount = 0
foreach ($record in (Get-AppEvent @{ LogName = 'Application'; Id = 1000; StartTime = $since })) {
    if ([string]$record.ProviderName -ne 'Application Error') {
        continue
    }
    $name = Get-FileName (Get-Value $record 0)
    if ($name.Length -eq 0) {
        continue
    }
    $crashCount++
    $entry = Get-AppEntry $name
    $entry.Crashes++
    if ($record.TimeCreated -gt $entry.Last) {
        $entry.Last = $record.TimeCreated
    }
    $module = Get-FileName (Get-Value $record 3)
    $code = (Get-Value $record 6).ToLowerInvariant()
    if ($code.StartsWith('0x')) {
        $code = $code.Substring(2)
    }
    $fault = $module + '|' + $code
    if ($entry.Faults.ContainsKey($fault)) {
        $entry.Faults[$fault]++
    }
    else {
        $entry.Faults[$fault] = 1
    }
}

$hangCount = 0
foreach ($record in (Get-AppEvent @{ LogName = 'Application'; Id = 1002; StartTime = $since })) {
    if ([string]$record.ProviderName -ne 'Application Hang') {
        continue
    }
    $name = Get-FileName (Get-Value $record 0)
    if ($name.Length -eq 0) {
        continue
    }
    $hangCount++
    $entry = Get-AppEntry $name
    $entry.Hangs++
    if ($record.TimeCreated -gt $entry.Last) {
        $entry.Last = $record.TimeCreated
    }
}

$sections = New-Object System.Collections.Generic.List[object]
$ordered = @($apps.Values | Sort-Object -Property @{ Expression = { $_.Crashes + $_.Hangs }; Descending = $true }, @{ Expression = { $_.Last }; Descending = $true } | Select-Object -First $maxApps)
foreach ($entry in $ordered) {
    $rows = New-Object System.Collections.Generic.List[object]
    if ($entry.Crashes -gt 0) {
        $rows.Add([ordered]@{ id = 'crashes'; value = [string]$entry.Crashes })
    }
    if ($entry.Hangs -gt 0) {
        $rows.Add([ordered]@{ id = 'hangs'; value = [string]$entry.Hangs })
    }
    $rows.Add([ordered]@{ id = 'last'; value = $entry.Last.ToString('yyyy-MM-dd HH:mm', [Globalization.CultureInfo]::InvariantCulture) })
    if ($entry.Faults.Count -gt 0) {
        $top = @($entry.Faults.GetEnumerator() | Sort-Object -Property Value -Descending)[0].Key
        $parts = $top.Split('|')
        $module = $parts[0]
        $code = $parts[1]
        if ($module.Length -gt 0) {
            $rows.Add([ordered]@{ id = 'module'; value = $module })
        }
        if ($code.Length -gt 0) {
            $rows.Add([ordered]@{ id = 'code'; value = $code })
        }
        $rows.Add([ordered]@{ id = 'cause'; code = (Get-Cause $module $code) })
    }
    else {
        $rows.Add([ordered]@{ id = 'cause'; code = 'hang' })
    }
    $sections.Add([ordered]@{ id = 'app'; name = $entry.Name; rows = $rows.ToArray() })
}

$result = 'none'
if ($sections.Count -gt 0) {
    $result = 'found'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{ days = $days; apps = $apps.Count; crashes = $crashCount; hangs = $hangCount }
    sections = $sections.ToArray()
}
