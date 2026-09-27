# Check: network.dns
# Can the system resolver turn well-known names into addresses? When it
# cannot, web pages do not open, while chat programs that connect by address
# (WeChat, QQ) may still work.
# Read-only. Uses the DNS servers Windows is set up with (never a server of
# our own) and asks a couple of well-known names that resolve from China,
# through the shared block dns-probe below: every name in a runspace of its
# own, the whole probe bounded to a few seconds.
#
# A failure is only blamed on DNS when the network itself is up:
#   offline      no default route at all (no router address): nothing to test
#                (unknown, "cannot test, the network is down")
#   ok           at least one name resolved
#   no-server    nothing resolved, and there is no DNS server on the adapters
#                that are up or the interfaces with a default route (or the
#                resolver says DNS_ERROR_NO_DNS_SERVERS)
#   failed-manual  nothing resolved and a DNS server was set by hand
#                (NameServer under Tcpip / Tcpip6 Parameters\Interfaces\<GUID>
#                of an adapter that is up)
#   failed       nothing resolved although Windows' own check (NCSI,
#                Get-NetConnectionProfile) says the internet is reachable
#   unreachable  nothing resolved and Windows says the internet is not
#                reachable either: the network is the problem, not DNS
#                (unknown, network.connectivity tells where it breaks)
# The script throws when the test cannot run at all (no Resolve-DnsName on a
# stripped system, no runspace and no answer).
#
# Facts (no addresses: the diagnostic report is shared): internet, has_route,
# server_count, manual_dns, tried, resolved, cached (names that were already
# in the DNS client cache), error (the most telling failure: no-servers /
# server-failure / not-found / bogus-answer / timeout / failed / not-run),
# error_code (the Win32 / DNS error number, 0 when there is none).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block dns-probe: identical in checks/network/connectivity.ps1 and checks/network/dns.ps1 (medkit-data check compares them) ----
# Small helpers used by both scripts.
function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

# A whole number, or -1 when the value is missing or not a number.
function Get-Number {
    param($Value)
    $n = [long]0
    if (($null -ne $Value) -and [long]::TryParse(([string]$Value).Trim(), [ref]$n)) {
        return $n
    }
    return [long]-1
}

# A registry value, or $null when the key or the value does not exist or
# cannot be read. '(default)' is the default value of the key.
function Get-RegistryValue {
    param([string]$Path, [string]$Name)
    $item = $null
    try {
        $item = Get-ItemProperty -LiteralPath $Path -ErrorAction Stop
    }
    catch {
        return $null
    }
    if ($null -eq $item) {
        return $null
    }
    $prop = $item.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    return $prop.Value
}

# IPv4Connectivity / IPv6Connectivity of a connection profile (the enum name
# or its number) as a fact value.
function ConvertTo-Connectivity {
    param($Value)
    $names = @{
        '0' = 'disconnected'; '1' = 'no-traffic'; '2' = 'subnet'; '3' = 'local-network'; '4' = 'internet'
        'disconnected' = 'disconnected'; 'notraffic' = 'no-traffic'; 'subnet' = 'subnet'
        'localnetwork' = 'local-network'; 'internet' = 'internet'
    }
    $text = (Get-Text $Value).ToLowerInvariant()
    if ($names.ContainsKey($text)) {
        return $names[$text]
    }
    return 'unknown'
}

# up / down / disabled / not-present, from the numbers of MSFT_NetAdapter
# (State 3 = disabled; InterfaceOperationalStatus 1 = up, 6 = not present),
# with the Status text of Get-NetAdapter as the fallback.
function Get-AdapterState {
    param($Adapter)
    $status = Get-Text $Adapter.Status
    $oper = Get-Number $Adapter.InterfaceOperationalStatus
    if (((Get-Number $Adapter.State) -eq 3) -or ($status -eq 'Disabled')) {
        return 'disabled'
    }
    if (($oper -eq 6) -or ($status -eq 'Not Present')) {
        return 'not-present'
    }
    if (($oper -eq 1) -or ($status -eq 'Up')) {
        return 'up'
    }
    if (($oper -lt 0) -and ($status.Length -eq 0) -and ((Get-Number $Adapter.MediaConnectState) -eq 1)) {
        return 'up'
    }
    return 'down'
}

# True when a DNS server is set by hand on one of these interfaces: the
# NameServer value of Tcpip (IPv4) or Tcpip6 (IPv6) under
# Parameters\Interfaces\<interface GUID>. Servers from DHCP are in
# DhcpNameServer instead.
function Test-ManualDnsServer {
    param([string[]]$Guids)
    $roots = @(
        'HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces',
        'HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip6\Parameters\Interfaces'
    )
    foreach ($guid in @($Guids)) {
        if ([string]::IsNullOrWhiteSpace($guid)) {
            continue
        }
        foreach ($root in $roots) {
            if ((Get-Text (Get-RegistryValue -Path ($root + '\' + $guid.Trim()) -Name 'NameServer')).Length -gt 0) {
                return $true
            }
        }
    }
    return $false
}

# DNS probe. Asks the system resolver (the DNS servers Windows is set up with;
# never a server of our own) for the A records of well-known names that
# resolve from China: Resolve-DnsName -DnsOnly -NoHostsFile -QuickTimeout. The
# hosts file is skipped so that the DNS servers themselves are tested. Names
# already in the DNS client cache (answers or failures) go last, because a
# cached entry says nothing about the servers right now.
# Every name is asked in a runspace of its own, so that they run at the same
# time and the probe is bounded: a name without an answer $dnsQueryBudgetMs
# after its query started, or $dnsProbeCapMs after the probe started, is
# abandoned (told to stop, not waited for), and so are the others as soon as
# one name has resolved (one answer is enough). One that was asked for at
# least $dnsMinQueryMs counts as a timeout; one that never really got going (a
# very slow runspace start) as not-run. When no runspace can be created, the
# names are asked one after the other in this runspace.
# Error per name: none (resolved) / timeout / server-failure / not-found /
# no-servers / bogus-answer (only 0.x or 127.x) / failed / unavailable (no
# Resolve-DnsName on this PC) / not-run.
$dnsCandidates = @('www.baidu.com', 'www.qq.com', 'www.msftconnecttest.com', 'www.163.com')
$dnsNamesPerProbe = 2
$dnsQueryBudgetMs = 2500
$dnsProbeCapMs = 4500
$dnsMinQueryMs = 1500

$dnsQueryScript = {
    param([string]$Name, [hashtable]$State)
    $ErrorActionPreference = 'Stop'
    # Loading the module can take a while on an old PC; the query clock starts
    # after it.
    Import-Module -Name DnsClient -ErrorAction SilentlyContinue
    $State['started'] = [DateTime]::UtcNow.Ticks
    try {
        $good = 0
        $bogus = 0
        foreach ($record in @(Resolve-DnsName -Name $Name -Type A -DnsOnly -NoHostsFile -QuickTimeout)) {
            if ([string]$record.Type -ne 'A') {
                continue
            }
            if ([string]$record.IPAddress -match '^(0|127)\.') {
                $bogus++
            }
            else {
                $good++
            }
        }
        if ($good -gt 0) {
            $State['resolved'] = $true
            return [pscustomobject]@{ Error = 'none'; Code = 0 }
        }
        if ($bogus -gt 0) {
            return [pscustomobject]@{ Error = 'bogus-answer'; Code = 0 }
        }
        return [pscustomobject]@{ Error = 'not-found'; Code = 0 }
    }
    catch {
        $code = 0
        $ex = $_.Exception
        while ($null -ne $ex) {
            if ($ex -is [System.ComponentModel.Win32Exception]) {
                $code = $ex.NativeErrorCode
                break
            }
            $ex = $ex.InnerException
        }
        $id = [string]$_.FullyQualifiedErrorId
        $kind = 'failed'
        if ($_.Exception -is [System.Management.Automation.CommandNotFoundException]) {
            $kind = 'unavailable'
        }
        elseif (($code -eq 9852) -or ($id -like 'DNS_ERROR_NO_DNS_SERVERS*')) {
            $kind = 'no-servers'
        }
        elseif (($code -eq 1460) -or ($id -like 'ERROR_TIMEOUT*')) {
            $kind = 'timeout'
        }
        elseif (($code -eq 9002) -or ($id -like 'DNS_ERROR_RCODE_SERVER_FAILURE*')) {
            $kind = 'server-failure'
        }
        elseif (($code -eq 9003) -or ($code -eq 9501) -or ($id -like 'DNS_ERROR_RCODE_NAME_ERROR*') -or ($id -like 'DNS_INFO_NO_RECORDS*')) {
            $kind = 'not-found'
        }
        return [pscustomobject]@{ Error = $kind; Code = $code }
    }
}

# The names to ask, uncached ones first; Cached = how many of them are in the
# DNS client cache.
function Select-DnsProbeName {
    $cached = @{}
    try {
        foreach ($entry in @(Get-DnsClientCache -ErrorAction Stop)) {
            $n = (Get-Text $entry.Entry).TrimEnd('.').ToLowerInvariant()
            if ($n.Length -gt 0) {
                $cached[$n] = $true
            }
        }
    }
    catch {
        $cached = @{}
    }
    $fresh = @($dnsCandidates | Where-Object { -not $cached.ContainsKey($_) })
    $stale = @($dnsCandidates | Where-Object { $cached.ContainsKey($_) })
    $names = @(@($fresh + $stale) | Select-Object -First $dnsNamesPerProbe)
    return [pscustomobject]@{
        Names  = $names
        Cached = @($names | Where-Object { $cached.ContainsKey($_) }).Count
    }
}

function Start-DnsProbe {
    param([string[]]$Names)
    $jobs = New-Object System.Collections.Generic.List[object]
    foreach ($name in $Names) {
        $state = [hashtable]::Synchronized(@{})
        $shell = $null
        $handle = $null
        try {
            $shell = [powershell]::Create()
            $null = $shell.AddScript($dnsQueryScript.ToString()).AddArgument($name).AddArgument($state)
            $handle = $shell.BeginInvoke()
        }
        catch {
            if ($null -ne $shell) {
                $shell.Dispose()
            }
            $shell = $null
            $handle = $null
        }
        $jobs.Add([pscustomobject]@{ Name = $name; Shell = $shell; Handle = $handle; State = $state })
    }
    return [pscustomobject]@{ Jobs = $jobs.ToArray(); Watch = [System.Diagnostics.Stopwatch]::StartNew() }
}

# Milliseconds since the query of a job started, -1 when it has not started.
function Get-DnsQueryAge {
    param($Job)
    $started = $Job.State['started']
    if ($null -eq $started) {
        return [long]-1
    }
    return [long](([DateTime]::UtcNow.Ticks - [long]$started) / 10000)
}

# One object per name: Name, Resolved, Error, Code.
function Receive-DnsProbe {
    param($Probe)
    while ($Probe.Watch.ElapsedMilliseconds -lt $dnsProbeCapMs) {
        $pending = 0
        foreach ($job in $Probe.Jobs) {
            if ($true -eq $job.State['resolved']) {
                $pending = 0
                break
            }
            if (($null -eq $job.Handle) -or $job.Handle.IsCompleted) {
                continue
            }
            if ((Get-DnsQueryAge $job) -ge $dnsQueryBudgetMs) {
                continue
            }
            $pending++
        }
        if ($pending -eq 0) {
            break
        }
        Start-Sleep -Milliseconds 50
    }
    $answers = New-Object System.Collections.Generic.List[object]
    foreach ($job in $Probe.Jobs) {
        $kind = 'not-run'
        $code = 0
        $out = @()
        if ($null -eq $job.Handle) {
            try {
                $out = @(& $dnsQueryScript $job.Name $job.State)
            }
            catch {
                $out = @([pscustomobject]@{ Error = 'failed'; Code = 0 })
            }
        }
        elseif ($job.Handle.IsCompleted -or (($true -eq $job.State['resolved']) -and $job.Handle.AsyncWaitHandle.WaitOne(1000))) {
            # A job that has just resolved may need a moment to finish.
            try {
                $out = @($job.Shell.EndInvoke($job.Handle))
            }
            catch {
                $out = @([pscustomobject]@{ Error = 'failed'; Code = 0 })
            }
            $job.Shell.Dispose()
        }
        else {
            if ((Get-DnsQueryAge $job) -ge $dnsMinQueryMs) {
                $kind = 'timeout'
            }
            $null = $job.Shell.BeginStop($null, $null)
        }
        if ($out.Count -gt 0) {
            $kind = Get-Text $out[$out.Count - 1].Error
            $code = [int](Get-Number $out[$out.Count - 1].Code)
            if ($kind.Length -eq 0) {
                $kind = 'failed'
            }
        }
        if ($true -eq $job.State['resolved']) {
            $kind = 'none'
            $code = 0
        }
        $answers.Add([pscustomobject]@{ Name = $job.Name; Resolved = ($kind -eq 'none'); Error = $kind; Code = $code })
    }
    return $answers.ToArray()
}
# ---- end of shared block dns-probe ----

$facts = [ordered]@{}

function Write-Result {
    param([string]$Result)
    [pscustomobject]@{
        result = $Result
        facts  = $facts
    }
}

# Windows' own verdict, only used to decide whether a failure is DNS's fault.
$internet = $false
try {
    foreach ($netProfile in @(Get-NetConnectionProfile -ErrorAction Stop)) {
        if (((ConvertTo-Connectivity $netProfile.IPv4Connectivity) -eq 'internet') -or ((ConvertTo-Connectivity $netProfile.IPv6Connectivity) -eq 'internet')) {
            $internet = $true
        }
    }
}
catch {
    $internet = $false
}
$facts['internet'] = $internet

# Without a default route there is no way out of the local network.
$routeIndexes = @{}
$routesKnown = $true
try {
    foreach ($route in @(Get-NetRoute -PolicyStore ActiveStore -ErrorAction Stop)) {
        $prefix = Get-Text $route.DestinationPrefix
        if (($prefix -eq '0.0.0.0/0') -or ($prefix -eq '::/0')) {
            $routeIndexes[[string](Get-Number $route.InterfaceIndex)] = $true
        }
    }
}
catch {
    $routesKnown = $false
}
if ($routesKnown) {
    $facts['has_route'] = ($routeIndexes.Count -gt 0)
    if (($routeIndexes.Count -eq 0) -and (-not $internet)) {
        Write-Result 'offline'
        return
    }
}

# Interfaces the resolver uses: adapters that are up, and interfaces with a
# default route (a PPPoE dial-up or VPN connection is not a visible adapter).
$active = @{}
foreach ($key in $routeIndexes.Keys) {
    $active[$key] = $true
}
$upGuids = New-Object System.Collections.Generic.List[string]
try {
    foreach ($adapter in @(Get-NetAdapter -ErrorAction Stop)) {
        if ((Get-AdapterState $adapter) -ne 'up') {
            continue
        }
        $active[[string](Get-Number $adapter.InterfaceIndex)] = $true
        $upGuids.Add((Get-Text $adapter.InterfaceGuid))
    }
}
catch {
    $upGuids.Clear()
}
$manualDns = Test-ManualDnsServer -Guids $upGuids.ToArray()
$facts['manual_dns'] = $manualDns

# DNS servers on those interfaces. fec0:0:0:ffff::1-3 are the placeholders
# Windows lists for IPv6 when no IPv6 DNS server is set; they do not count.
$serverCount = -1
try {
    if ($active.Count -eq 0) {
        throw 'No interface to count DNS servers on'
    }
    $serverCount = 0
    foreach ($entry in @(Get-DnsClientServerAddress -ErrorAction Stop)) {
        if (-not $active.ContainsKey([string](Get-Number $entry.InterfaceIndex))) {
            continue
        }
        foreach ($server in @($entry.ServerAddresses)) {
            $text = Get-Text $server
            if (($text.Length -gt 0) -and (-not $text.ToLowerInvariant().StartsWith('fec0:'))) {
                $serverCount++
            }
        }
    }
}
catch {
    $serverCount = -1
}
if ($serverCount -ge 0) {
    $facts['server_count'] = $serverCount
}

# The count is only supporting evidence (an interface may be missed), so the
# names are always asked.
$selection = Select-DnsProbeName
$answers = @(Receive-DnsProbe -Probe (Start-DnsProbe -Names $selection.Names))
$resolved = @($answers | Where-Object { $_.Resolved })
$facts['tried'] = $answers.Count
$facts['resolved'] = $resolved.Count
$facts['cached'] = $selection.Cached

if ($resolved.Count -gt 0) {
    Write-Result 'ok'
    return
}

# The most telling failure goes into the facts.
$worst = $null
foreach ($kind in @('no-servers', 'server-failure', 'not-found', 'bogus-answer', 'timeout', 'failed', 'not-run', 'unavailable')) {
    $worst = @($answers | Where-Object { $_.Error -eq $kind }) | Select-Object -First 1
    if ($null -ne $worst) {
        break
    }
}
if ($null -ne $worst) {
    $facts['error'] = $worst.Error
    $facts['error_code'] = $worst.Code
}

$noServers = (@($answers | Where-Object { $_.Error -eq 'no-servers' }).Count -gt 0)
$ran = @($answers | Where-Object { ($_.Error -ne 'not-run') -and ($_.Error -ne 'unavailable') })
if (($ran.Count -eq 0) -and ($serverCount -ne 0)) {
    if (@($answers | Where-Object { $_.Error -eq 'unavailable' }).Count -gt 0) {
        throw 'Resolve-DnsName is not available on this computer'
    }
    throw 'The DNS test could not run'
}

if ($noServers -or ($serverCount -eq 0)) {
    Write-Result 'no-server'
}
elseif ($manualDns) {
    Write-Result 'failed-manual'
}
elseif ($internet) {
    Write-Result 'failed'
}
else {
    Write-Result 'unreachable'
}
