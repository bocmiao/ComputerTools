# Check: system.busy
# What keeps the PC busy right now. Read-only: nothing is ended or changed.
# Processor: the processor time of every process is read twice (Get-Process,
# property CPU), $sampleSeconds apart, and added up by program (all
# chrome.exe processes count as one; every svchost.exe is its own program,
# named after the Windows services in it). A process that started in between
# counts with all of its processor time. Use = processor time spent in the
# interval / interval / logical processors. (The cooked WMI performance
# classes need a refresher object that PowerShell does not have, and the
# performance counter names are localized, so neither is used.)
# Memory: Win32_OperatingSystem (TotalVisibleMemorySize, FreePhysicalMemory),
# and the working set of every program at the second reading.
# Result codes, in this order:
#   when the processor is at least $busyPercent busy, the busiest program
#   decides: defender (Windows Security scanning), antivirus (another security
#   program), update (installing updates), indexing (Windows Search), browser,
#   explorer (the desktop and folder windows), system (a part of Windows that
#   cannot be closed), other
#   memory-full   at least $fullPercent of the memory is in use
#   ok
# Facts: cpu_pct, memory_pct, busiest (name of the busiest program),
# top_cpu (up to 3 "name 35%"), top_memory (up to 3 "name 1.2 GB").
# Names are the file description of the program (what Task Manager shows),
# or the process name when there is none. No window titles or command lines.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$sampleSeconds = 8
$busyPercent = 60
$fullPercent = 90
# Process names (lower case, without .exe) -> result code.
$kinds = @{
    'msmpeng'               = 'defender'
    'mpdefendercoreservice' = 'defender'
    'nissrv'                = 'defender'
    'mpcmdrun'              = 'defender'
    'mssense'               = 'defender'
    '360tray'               = 'antivirus'
    '360sd'                 = 'antivirus'
    '360rp'                 = 'antivirus'
    'zhudongfangyu'         = 'antivirus'
    'qqpctray'              = 'antivirus'
    'qqpcrtp'               = 'antivirus'
    'hipsdaemon'            = 'antivirus'
    'hipstray'              = 'antivirus'
    'usysdiag'              = 'antivirus'
    'kxetray'               = 'antivirus'
    'kxescore'              = 'antivirus'
    'avp'                   = 'antivirus'
    'ekrn'                  = 'antivirus'
    'avastsvc'              = 'antivirus'
    'mcshield'              = 'antivirus'
    'tiworker'              = 'update'
    'trustedinstaller'      = 'update'
    'mousocoreworker'       = 'update'
    'usocoreworker'         = 'update'
    'wuauclt'               = 'update'
    'sihclient'             = 'update'
    'dismhost'              = 'update'
    'searchindexer'         = 'indexing'
    'searchprotocolhost'    = 'indexing'
    'searchfilterhost'      = 'indexing'
    'msedge'                = 'browser'
    'chrome'                = 'browser'
    'firefox'               = 'browser'
    '360se'                 = 'browser'
    '360chrome'             = 'browser'
    '360chromex'            = 'browser'
    'qqbrowser'             = 'browser'
    'sogouexplorer'         = 'browser'
    '2345explorer'          = 'browser'
    'opera'                 = 'browser'
    'brave'                 = 'browser'
    'explorer'              = 'explorer'
    'system'                = 'system'
    'registry'              = 'system'
    'memory compression'    = 'system'
    'secure system'         = 'system'
    'smss'                  = 'system'
    'csrss'                 = 'system'
    'wininit'               = 'system'
    'winlogon'              = 'system'
    'services'              = 'system'
    'lsass'                 = 'system'
    'dwm'                   = 'system'
    'wmiprvse'              = 'system'
    'audiodg'               = 'system'
    'fontdrvhost'           = 'system'
    'svchost'               = 'system'
}
# Windows services (lower case) that download or install updates: a svchost.exe
# running one of them is Windows Update at work.
$updateServices = @('wuauserv', 'usosvc', 'dosvc', 'bits', 'waasmedicsvc')
# Parts of the kernel that Task Manager lists as processes. Their memory is not
# a program's (Memory Compression holds other programs' memory), so they are
# left out of the memory list.
$kernelNames = @('system', 'registry', 'memory compression', 'secure system')

# Processor time used so far, in seconds, or $null when it cannot be read.
function Get-ProcessorTime {
    param($Process)
    try {
        $cpu = $Process.CPU
    }
    catch {
        return $null
    }
    if ($null -eq $cpu) {
        return $null
    }
    return [double]$cpu
}

# Process id -> @{ Name; Cpu } for every process, Cpu $null when unreadable.
function Read-ProcessTime {
    $times = @{}
    foreach ($process in @(Get-Process -ErrorAction SilentlyContinue)) {
        $times[$process.Id] = @{ Name = [string]$process.ProcessName; Cpu = (Get-ProcessorTime $process) }
    }
    return $times
}

# What Task Manager shows for a program: the file description of its
# executable, or the process name.
function Get-FileDescription {
    param([string]$Name, $Process)
    try {
        $path = [string]$Process.Path
        if ($path.Length -gt 0) {
            $description = ([string][System.Diagnostics.FileVersionInfo]::GetVersionInfo($path).FileDescription).Trim()
            if ($description.Length -gt 0) {
                return $description
            }
        }
    }
    catch {
        Write-Verbose ('No file description for {0}: {1}' -f $Name, $_.Exception.Message)
    }
    return $Name
}

# The Windows services running in one svchost.exe.
function Get-HostedService {
    param([int]$ProcessId)
    try {
        return @(Get-CimInstance -ClassName Win32_Service -Filter ('ProcessId = {0}' -f $ProcessId) -Property Name, DisplayName)
    }
    catch {
        Write-Verbose ('No services for process {0}: {1}' -f $ProcessId, $_.Exception.Message)
        return @()
    }
}

# Fills in the display name and the result code of a program, once.
function Resolve-Program {
    param($Program)
    if ($null -ne $Program.Display) {
        return
    }
    $key = $Program.Name.ToLowerInvariant()
    $display = Get-FileDescription $Program.Name $Program.Sample
    $kind = $kinds[$key]
    if ($key -eq 'svchost') {
        $services = @(Get-HostedService $Program.Sample.Id)
        if ($services.Count -gt 0) {
            $names = @($services | Select-Object -First 3 | ForEach-Object { [string]$_.DisplayName })
            if ($services.Count -gt 3) {
                $names += '...'
            }
            $display = '{0} ({1})' -f $display, ($names -join ', ')
            foreach ($service in $services) {
                if ($updateServices -contains ([string]$service.Name).ToLowerInvariant()) {
                    $kind = 'update'
                }
            }
        }
    }
    if ($null -eq $kind) {
        $kind = 'other'
    }
    $Program.Display = $display
    $Program.Kind = $kind
}

$first = Read-ProcessTime
$watch = [System.Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds $sampleSeconds
$processes = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne 0 })
$elapsed = [math]::Max($watch.Elapsed.TotalSeconds, 1)
$cores = [math]::Max([Environment]::ProcessorCount, 1)

# Program key -> totals, and one process to read the name from.
$programs = @{}
$totalCpu = [double]0
foreach ($process in $processes) {
    $name = [string]$process.ProcessName
    $key = $name.ToLowerInvariant()
    # Every svchost.exe runs different Windows services: each is its own program.
    if ($key -eq 'svchost') {
        $key = 'svchost#{0}' -f $process.Id
    }
    if (-not $programs.ContainsKey($key)) {
        $programs[$key] = @{ Name = $name; Cpu = [double]0; Memory = [double]0; Sample = $process; Display = $null; Kind = $null }
    }
    $programs[$key].Memory += [double]$process.WorkingSet64
    $cpu = Get-ProcessorTime $process
    if ($null -eq $cpu) {
        continue
    }
    $before = $first[$process.Id]
    $used = [double]0
    if ($null -eq $before) {
        # Started during the interval: all of its processor time is new.
        $used = $cpu
    }
    elseif (($before.Name -eq $name) -and ($null -ne $before.Cpu)) {
        $used = [math]::Max($cpu - $before.Cpu, 0)
    }
    $programs[$key].Cpu += $used
    $totalCpu += $used
}

$cpuPercent = [math]::Min(100, $totalCpu / $elapsed / $cores * 100)
$os = Get-CimInstance -ClassName Win32_OperatingSystem
$memoryPercent = 0
if ([double]$os.TotalVisibleMemorySize -gt 0) {
    $memoryPercent = (1 - ([double]$os.FreePhysicalMemory / [double]$os.TotalVisibleMemorySize)) * 100
}

$byCpu = @($programs.Values | Sort-Object -Property { $_.Cpu } -Descending)
$byMemory = @($programs.Values |
        Where-Object { $kernelNames -notcontains $_.Name.ToLowerInvariant() } |
        Sort-Object -Property { $_.Memory } -Descending)
$topCpu = New-Object System.Collections.Generic.List[string]
foreach ($program in @($byCpu | Select-Object -First 3)) {
    $percent = $program.Cpu / $elapsed / $cores * 100
    if ($percent -ge 1) {
        Resolve-Program $program
        $topCpu.Add(('{0} {1:0}%' -f $program.Display, $percent))
    }
}
$topMemory = New-Object System.Collections.Generic.List[string]
foreach ($program in @($byMemory | Select-Object -First 3)) {
    Resolve-Program $program
    $topMemory.Add(('{0} {1:0.0} GB' -f $program.Display, ($program.Memory / 1GB)))
}

$result = 'ok'
$busiest = ''
if (($cpuPercent -ge $busyPercent) -and ($byCpu.Count -gt 0)) {
    Resolve-Program $byCpu[0]
    $busiest = $byCpu[0].Display
    $result = $byCpu[0].Kind
}
elseif ($memoryPercent -ge $fullPercent) {
    $result = 'memory-full'
    if ($byMemory.Count -gt 0) {
        $busiest = $byMemory[0].Display
    }
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        cpu_pct    = [math]::Round($cpuPercent)
        memory_pct = [math]::Round($memoryPercent)
        busiest    = $busiest
        top_cpu    = $topCpu.ToArray()
        top_memory = $topMemory.ToArray()
    }
}
