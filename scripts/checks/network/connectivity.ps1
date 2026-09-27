# Check: network.connectivity
# Walks the network chain in order and reports the first link that is broken,
# as one result code, so that the UI can show one clear conclusion:
#   network adapter (driver, disabled, airplane mode) -> link (cable / WiFi)
#   -> address from the router (169.254.x.x means none) -> router (default
#   gateway) -> internet.
# Read-only; works without any network at all. Nothing is sent over the
# network except an ICMP echo to the router and DNS queries through the
# system resolver (no HTTP, no server of our own).
#
# 1. Windows' own verdict first. Get-NetConnectionProfile reports what the
#    Network Connectivity Status Indicator (NCSI) found for every connected
#    network: IPv4Connectivity / IPv6Connectivity = Disconnected, NoTraffic,
#    Subnet, LocalNetwork or Internet (MSFT_NetConnectionProfile, values 0-4).
#    Internet on any connection -> ok, and nothing else is probed.
# 2. Adapters (Get-NetAdapter, visible adapters). Physical means
#    HardwareInterface or ConnectorPresent ("TRUE if this is a physical
#    adapter", MSFT_NetAdapter), not EndPointInterface; Bluetooth PAN adapters
#    (NdisPhysicalMedium 10) are left out, they are not how people get online.
#    Kind: wifi (NdisPhysicalMedium 9 / 1, ifType 71), mobile (8, ifType
#    243 / 244), ethernet (14, or 0 with ifType 6), other. State: disabled
#    (State 3), not present (InterfaceOperationalStatus 6, ignored), up
#    (InterfaceOperationalStatus 1), otherwise down; the Status text
#    ("Up", "Disabled", "Not Present") is the fallback.
# 3. No physical adapter is up:
#    - a network card without a working driver (Win32_PnPEntity with a problem
#      code; recognised with the same rules as hardware.device-problems, see
#      Find-NetworkProblemDevice) -> no-driver;
#    - no physical adapter at all -> no-adapter;
#    - airplane mode on and a wireless adapter that is not disabled ->
#      airplane-mode. Airplane mode is the default value of the key
#      HKLM\SYSTEM\CurrentControlSet\Control\RadioManagement\SystemRadioState
#      (DWORD, 1 = on, 0 = off). Microsoft does not document it; community
#      guides and a Microsoft forum moderator point at it. A value named
#      SystemRadioState under RadioManagement is read as a fallback. It is only
#      used when nothing is connected: Windows lets people turn WiFi back on
#      while airplane mode stays on;
#    - a physical adapter is disabled -> adapter-disabled;
#    - otherwise not-connected-wifi (only wireless adapters),
#      not-connected-cable (only ethernet) or not-connected (both, or other).
# 4. Adapters that are up: the physical ones, plus virtual ones that carry an
#    IPv4 default route through a real router (Hyper-V external switch,
#    network bridge). For each: a usable IPv4 address (not 169.254.x.x APIPA,
#    not Duplicate / Invalid), whether it was set by hand (PrefixOrigin
#    Manual), the IPv4 default gateway (Get-NetRoute 0.0.0.0/0 in the active
#    store; next hop 0.0.0.0 means on-link, as on PPP or mobile broadband),
#    and an IPv6 default route with a global address. Windows' own dial-up
#    (PPPoE) and VPN connections are not adapters; one with an address and an
#    IPv4 default route counts as well (kind dial-up, "router" on-link). The
#    adapter that gets furthest along the chain is the one the result is about.
# 5. When that adapter has a gateway (or any adapter has IPv6), the router
#    and the way beyond it are tested at the same time:
#    - ping the gateway: System.Net.NetworkInformation.Ping, 3 tries of 700 ms.
#      Some routers do not answer ping, so when it gets no answer the neighbor
#      cache is read too (Get-NetNeighbor): Reachable / Permanent means the
#      router answered ARP;
#    - DNS through the system resolver (shared block dns-probe below). A DNS
#      answer means the router passes traffic, whatever the ping said.
#    NCSI also reports "no internet" when only DNS is broken (its probe
#    starts with a DNS lookup) and when its probe goes through a dead proxy,
#    so Windows' verdict alone is not enough to blame the broadband:
#    - DNS answers -> internet-unconfirmed (unknown: a web login page, a
#      leftover proxy, an overdue broadband bill that redirects web pages...),
#      or ncsi-off when NCSI's active test is switched off or changed
#      (EnableActiveProbing = 0 or ActiveWebProbeHost changed under
#      HKLM\SYSTEM\CurrentControlSet\Services\NlaSvc\Parameters\Internet, or
#      the policy HKLM\SOFTWARE\Policies\Microsoft\Windows\
#      NetworkConnectivityStatusIndicator\NoActiveProbe = 1);
#    - no usable IPv4 address -> no-address;
#    - an address but no gateway -> manual-address (set by hand) or no-gateway;
#    - the router answers neither ping nor ARP -> manual-address (set by hand)
#      or gateway-unreachable;
#    - the router answers, DNS does not, and a DNS server was set by hand (or
#      there is none) -> dns-suspect (unknown: the next check, network.dns,
#      tells whether DNS is the problem);
#    - the router answers but the DNS test could not run at all (no
#      Resolve-DnsName, or a PC too slow to start it in time) -> dns-untested
#      (unknown): Windows' verdict alone may come from a dead proxy, so the
#      broadband is not blamed and the symptom goes on to the proxy check;
#    - the router answers, DNS through it does not either -> no-internet: the
#      broadband or the modem, not this PC.
# Results that stop the symptom (advice / manual) are only those where the
# user has something to do; the unknown ones let network.dns and
# network.proxy-dead run next.
#
# Facts (booleans, counts and enums only: no IP or MAC address, SSID, adapter
# GUID or computer name, because the diagnostic report is shared; the adapter
# description such as "Intel(R) Wi-Fi 6 AX201 160MHz" is fine):
#   ncsi_available, ipv4_connectivity, ipv6_connectivity (internet /
#   local-network / subnet / no-traffic / disconnected / unknown / none),
#   adapter_count, wifi_count, ethernet_count, connected_count,
#   disabled_count, airplane_mode, adapter, adapter_kind (wifi / ethernet /
#   mobile / dial-up / other), wifi_low_power, problem_device, problem_code, apipa,
#   address_conflict, manual_address, has_gateway, gateway_ping, gateway_arp,
#   gateway_reachable, manual_dns, dns (ok / failed / no-servers / unknown),
#   ncsi_probe_off.

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

# Gateway ping: a few short tries (a router answers within milliseconds).
$pingTries = 3
$pingTimeoutMs = 700

$netClassGuid = '{4d36e972-e325-11ce-bfc1-08002be10318}'
$ignoredDeviceCodes = @(22, 29, 45, 47, 53)
# PCI base class 02 (network controller); USB CDC NCM / ECM / MBIM and RNDIS.
$networkIdPattern = '(?i)^(PCI\\(.*&)?CC_02|USB\\CLASS_02&SUBCLASS_(06|0D|0E)|USB\\CLASS_EF&SUBCLASS_04&PROT_01|USB\\CLASS_E0&SUBCLASS_01&PROT_03)'
# The Chinese words for network card, network controller and Ethernet as \u escapes.
$networkNamePattern = '(?i)\b(wi-?fi|wlan|wireless (lan|network)|ethernet|gbe|lan (adapter|card)|network (adapter|controller|card|connection))\b|802\.11|\u7f51\u5361|\u7f51\u7edc\u63a7\u5236\u5668|\u4ee5\u592a\u7f51'
# "Bluetooth", and the Chinese word for it.
$bluetoothNamePattern = '(?i)bluetooth|\u84dd\u7259'
$radioKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\RadioManagement\SystemRadioState'
$radioParentKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\RadioManagement'
$ncsiKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\NlaSvc\Parameters\Internet'
$ncsiPolicyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\NetworkConnectivityStatusIndicator'
$defaultProbeHost = 'www.msftconnecttest.com'

function Test-True {
    param($Value)
    if ($null -eq $Value) {
        return $false
    }
    if ($Value -is [bool]) {
        return $Value
    }
    $t = (Get-Text $Value).ToLowerInvariant()
    return (($t -eq 'true') -or ($t -eq '1'))
}

# wifi / mobile / ethernet / bluetooth / other.
function Get-AdapterKind {
    param($Adapter)
    $medium = Get-Number $Adapter.NdisPhysicalMedium
    $ifType = Get-Number $Adapter.InterfaceType
    $mediaText = Get-Text $Adapter.PhysicalMediaType
    if (($medium -eq 10) -or ($mediaText -match 'Bluetooth')) {
        return 'bluetooth'
    }
    if (($medium -eq 9) -or ($medium -eq 1) -or ($ifType -eq 71) -or ($mediaText -match '802\.11|Wireless LAN')) {
        return 'wifi'
    }
    if (($medium -eq 8) -or ($ifType -eq 243) -or ($ifType -eq 244) -or ($mediaText -match 'Wireless WAN')) {
        return 'mobile'
    }
    if (($medium -eq 14) -or (($medium -eq 0) -and ($ifType -eq 6)) -or ($mediaText -eq '802.3')) {
        return 'ethernet'
    }
    return 'other'
}

function Test-PhysicalAdapter {
    param($Adapter)
    if (Test-True $Adapter.EndPointInterface) {
        return $false
    }
    return ((Test-True $Adapter.HardwareInterface) -or (Test-True $Adapter.ConnectorPresent))
}

# $true / $false, or $null when the value cannot be read.
function Get-AirplaneMode {
    $value = Get-RegistryValue -Path $radioKey -Name '(default)'
    if ($null -eq $value) {
        $value = Get-RegistryValue -Path $radioParentKey -Name 'SystemRadioState'
    }
    $n = Get-Number $value
    if ($n -eq 1) {
        return $true
    }
    if ($n -eq 0) {
        return $false
    }
    return $null
}

# The network card whose driver is missing (codes 1, 28; preferred) or not
# working (any other code), as @{ Name; Code }, or $null. Network cards are
# recognised with the same rules as hardware.device-problems: on a hardware
# bus (PCI, USB, SD; VPN and virtual adapters are ROOT / SWD), not named
# Bluetooth, and in the Net class, with a compatible ID of PCI base class 02 or
# USB CDC NCM / ECM / MBIM / RNDIS, or with a name that says Wi-Fi, WLAN,
# 802.11, Ethernet ... (USB Wi-Fi sticks often have nothing else). Not counted:
# 45 not connected, 47 prepared for removal, 53 kernel debugger, 22 / 29
# disabled (in Windows or in the BIOS), and devices with Present = false.
function Find-NetworkProblemDevice {
    $devices = @()
    try {
        $devices = @(Get-CimInstance -ClassName Win32_PnPEntity -Filter 'ConfigManagerErrorCode <> 0 AND ConfigManagerErrorCode <> 45' -ErrorAction Stop)
    }
    catch {
        return $null
    }
    $found = $null
    foreach ($device in $devices) {
        if (($null -eq $device) -or ($device.Present -eq $false)) {
            continue
        }
        $code = Get-Number $device.ConfigManagerErrorCode
        if (($code -le 0) -or ($ignoredDeviceCodes -contains $code)) {
            continue
        }
        $pnpId = Get-Text $device.PNPDeviceID
        if ($pnpId -notmatch '^(PCI|USB|SD)\\') {
            continue
        }
        $name = Get-Text $device.Name
        if ($name -match $bluetoothNamePattern) {
            continue
        }
        $isNet = (((Get-Text $device.PNPClass) -eq 'Net') -or ((Get-Text $device.ClassGuid) -eq $netClassGuid))
        if (-not $isNet) {
            foreach ($id in @(@($device.CompatibleID) + @($device.HardwareID))) {
                if ((Get-Text $id) -match $networkIdPattern) {
                    $isNet = $true
                    break
                }
            }
        }
        if ((-not $isNet) -and ($name -match $networkNamePattern)) {
            $isNet = $true
        }
        if (-not $isNet) {
            continue
        }
        if ($name.Length -eq 0) {
            $name = 'Network controller'
        }
        $candidate = [pscustomobject]@{ Name = $name; Code = $code }
        if (($code -eq 1) -or ($code -eq 28)) {
            return $candidate
        }
        if ($null -eq $found) {
            $found = $candidate
        }
    }
    return $found
}

# True when NCSI's active test is switched off or points somewhere else, so
# that its "no internet" says little.
function Test-NcsiProbeOff {
    if ((Get-Number (Get-RegistryValue -Path $ncsiKey -Name 'EnableActiveProbing')) -eq 0) {
        return $true
    }
    if ((Get-Number (Get-RegistryValue -Path $ncsiPolicyKey -Name 'NoActiveProbe')) -eq 1) {
        return $true
    }
    $probeHost = Get-Text (Get-RegistryValue -Path $ncsiKey -Name 'ActiveWebProbeHost')
    return (($probeHost.Length -gt 0) -and ($probeHost -ne $defaultProbeHost))
}

function Test-GatewayPing {
    param([string]$Address)
    $ping = $null
    try {
        $ping = New-Object System.Net.NetworkInformation.Ping
        for ($i = 0; $i -lt $pingTries; $i++) {
            try {
                $reply = $ping.Send($Address, $pingTimeoutMs)
                if (($null -ne $reply) -and ((Get-Text $reply.Status) -eq 'Success')) {
                    return $true
                }
            }
            catch {
                # No route, no resources: the same as no answer. Wait a moment
                # so that the tries do not all fail at once.
                Start-Sleep -Milliseconds 100
            }
        }
    }
    finally {
        if ($null -ne $ping) {
            $ping.Dispose()
        }
    }
    return $false
}

# True when the neighbor cache says the router answered ARP just now.
function Test-GatewayNeighbor {
    param([string]$Address, [long]$Index)
    try {
        foreach ($neighbor in @(Get-NetNeighbor -InterfaceIndex $Index -AddressFamily IPv4 -ErrorAction Stop)) {
            if ((Get-Text $neighbor.IPAddress) -ne $Address) {
                continue
            }
            $state = (Get-Text $neighbor.State).ToLowerInvariant()
            return (($state -eq 'reachable') -or ($state -eq 'permanent') -or ($state -eq '5') -or ($state -eq '6'))
        }
    }
    catch {
        return $false
    }
    return $false
}

function Get-AddressState {
    param($Address)
    $names = @{ '0' = 'invalid'; '1' = 'tentative'; '2' = 'duplicate'; '3' = 'deprecated'; '4' = 'preferred' }
    $text = (Get-Text $Address.AddressState).ToLowerInvariant()
    if ($names.ContainsKey($text)) {
        return $names[$text]
    }
    return $text
}

function Test-ManualPrefix {
    param($Address)
    $origin = (Get-Text $Address.PrefixOrigin).ToLowerInvariant()
    return (($origin -eq 'manual') -or ($origin -eq '1'))
}

# How far one adapter that is up gets along the chain.
# V4Stage: 0 no usable IPv4 address, 1 an address but no gateway, 2 a gateway.
function Get-InterfaceChain {
    param($Info)
    $key = [string]$Info.Index
    $v4Valid = $false
    $apipa = $false
    $conflict = $false
    $manual = $false
    $v6Global = $false
    $own = @()
    if ($addressesByIndex.ContainsKey($key)) {
        $own = $addressesByIndex[$key].ToArray()
    }
    foreach ($address in $own) {
        $ip = Get-Text $address.IPAddress
        $state = Get-AddressState $address
        if ($ip.Contains(':')) {
            if (($ip -match '^[23][0-9a-fA-F]{0,3}:') -and ($state -ne 'duplicate') -and ($state -ne 'invalid')) {
                $v6Global = $true
            }
            continue
        }
        if ($ip.StartsWith('169.254.')) {
            $apipa = $true
            continue
        }
        if ($ip.StartsWith('127.') -or ($ip.Length -eq 0)) {
            continue
        }
        if ($state -eq 'duplicate') {
            $conflict = $true
            continue
        }
        if ($state -eq 'invalid') {
            continue
        }
        $v4Valid = $true
        if (Test-ManualPrefix $address) {
            $manual = $true
        }
    }
    $gateway = ''
    $onLink = $false
    if ($v4Routes.ContainsKey($key)) {
        foreach ($hop in $v4Routes[$key].ToArray()) {
            if (($hop.Length -gt 0) -and ($hop -ne '0.0.0.0')) {
                $gateway = $hop
                break
            }
        }
        if ($gateway.Length -eq 0) {
            $onLink = $true
        }
    }
    $stage = 0
    if ($v4Valid) {
        $stage = 1
        if (($gateway.Length -gt 0) -or $onLink) {
            $stage = 2
        }
    }
    return [pscustomobject]@{
        Info     = $Info
        V4Stage  = $stage
        Apipa    = $apipa
        Conflict = $conflict
        Manual   = $manual
        Gateway  = $gateway
        OnLink   = $onLink
        V6Route  = ($v6Global -and $v6Routes.ContainsKey($key))
    }
}

# Higher is further along the chain; physical adapters and WiFi / ethernet
# first when two get equally far.
function Get-ChainScore {
    param($Chain)
    $score = $Chain.V4Stage * 100
    if ($Chain.V6Route) {
        $score += 50
    }
    if ($Chain.Info.Physical) {
        $score += 10
    }
    if (($Chain.Info.Kind -eq 'wifi') -or ($Chain.Info.Kind -eq 'ethernet')) {
        $score += 5
    }
    if ($profileByIndex.ContainsKey([string]$Chain.Info.Index)) {
        $score += 1
    }
    return $score
}

function Write-Result {
    param([string]$Result)
    [pscustomobject]@{
        result = $Result
        facts  = $facts
    }
}

$facts = [ordered]@{}

# ---- 1. Windows' own verdict.
$ncsiAvailable = $true
$netProfiles = @()
try {
    $netProfiles = @(Get-NetConnectionProfile -ErrorAction Stop)
}
catch {
    $ncsiAvailable = $false
}
$facts['ncsi_available'] = $ncsiAvailable
$profileByIndex = @{}
$internetProfile = $null
foreach ($netProfile in $netProfiles) {
    $entry = [pscustomobject]@{
        Index = Get-Number $netProfile.InterfaceIndex
        V4    = ConvertTo-Connectivity $netProfile.IPv4Connectivity
        V6    = ConvertTo-Connectivity $netProfile.IPv6Connectivity
    }
    if ($entry.Index -ge 0) {
        $profileByIndex[[string]$entry.Index] = $entry
    }
    if (($null -eq $internetProfile) -and (($entry.V4 -eq 'internet') -or ($entry.V6 -eq 'internet'))) {
        $internetProfile = $entry
    }
}

# ---- 2. Adapters.
$adapters = @()
$adapterError = ''
try {
    $adapters = @(Get-NetAdapter -ErrorAction Stop)
}
catch {
    $adapterError = $_.Exception.Message
}
$infos = New-Object System.Collections.Generic.List[object]
foreach ($adapter in $adapters) {
    $kind = Get-AdapterKind $adapter
    $index = Get-Number $adapter.InterfaceIndex
    if ($index -lt 0) {
        $index = Get-Number $adapter.ifIndex
    }
    # The description comes from the driver. The adapter name ("WLAN") can be
    # renamed by the user, so it never goes into the facts.
    $description = Get-Text $adapter.InterfaceDescription
    if ($description.Length -eq 0) {
        $description = 'Network adapter'
    }
    $infos.Add([pscustomobject]@{
            Index       = $index
            Guid        = Get-Text $adapter.InterfaceGuid
            Description = $description
            Kind        = $kind
            State       = Get-AdapterState $adapter
            Physical    = ((Test-PhysicalAdapter $adapter) -and ($kind -ne 'bluetooth'))
            LowPower    = Test-True $adapter.OperationalStatusDownLowPowerState
        })
}

if ($null -ne $internetProfile) {
    $facts['ipv4_connectivity'] = $internetProfile.V4
    $facts['ipv6_connectivity'] = $internetProfile.V6
    foreach ($info in $infos) {
        if ($info.Index -eq $internetProfile.Index) {
            $facts['adapter'] = $info.Description
            $facts['adapter_kind'] = $info.Kind
            break
        }
    }
    Write-Result 'ok'
    return
}
if ($adapterError.Length -gt 0) {
    throw ('Cannot list the network adapters: {0}' -f $adapterError)
}

$physical = @($infos | Where-Object { $_.Physical -and ($_.State -ne 'not-present') })
$wireless = @($physical | Where-Object { ($_.Kind -eq 'wifi') -or ($_.Kind -eq 'mobile') })
$wired = @($physical | Where-Object { $_.Kind -eq 'ethernet' })
$up = @($physical | Where-Object { $_.State -eq 'up' })
$disabled = @($physical | Where-Object { $_.State -eq 'disabled' })
$facts['adapter_count'] = $physical.Count
$facts['wifi_count'] = @($physical | Where-Object { $_.Kind -eq 'wifi' }).Count
$facts['ethernet_count'] = $wired.Count
$facts['connected_count'] = $up.Count
$facts['disabled_count'] = $disabled.Count
$airplane = $null
if ($wireless.Count -gt 0) {
    $airplane = Get-AirplaneMode
    if ($null -ne $airplane) {
        $facts['airplane_mode'] = $airplane
    }
}

# ---- 3. Nothing is connected.
if ($up.Count -eq 0) {
    $problem = Find-NetworkProblemDevice
    $enabledWireless = @($wireless | Where-Object { $_.State -ne 'disabled' })
    $enabledWired = @($wired | Where-Object { $_.State -ne 'disabled' })
    if ($null -ne $problem) {
        $facts['problem_device'] = $problem.Name
        $facts['problem_code'] = $problem.Code
        Write-Result 'no-driver'
    }
    elseif ($physical.Count -eq 0) {
        Write-Result 'no-adapter'
    }
    elseif (($airplane -eq $true) -and ($enabledWireless.Count -gt 0)) {
        Write-Result 'airplane-mode'
    }
    elseif ($disabled.Count -gt 0) {
        $facts['adapter'] = $disabled[0].Description
        $facts['adapter_kind'] = $disabled[0].Kind
        Write-Result 'adapter-disabled'
    }
    elseif (($enabledWireless.Count -gt 0) -and ($enabledWired.Count -eq 0) -and ($enabledWireless.Count -eq $physical.Count)) {
        $facts['wifi_low_power'] = (@($enabledWireless | Where-Object { $_.LowPower }).Count -gt 0)
        Write-Result 'not-connected-wifi'
    }
    elseif (($enabledWired.Count -gt 0) -and ($enabledWireless.Count -eq 0) -and ($enabledWired.Count -eq $physical.Count)) {
        Write-Result 'not-connected-cable'
    }
    else {
        Write-Result 'not-connected'
    }
    return
}

# ---- 4. Addresses and routes of the adapters that are up.
$addresses = @()
$routes = @()
try {
    $addresses = @(Get-NetIPAddress -ErrorAction Stop)
    $routes = @(Get-NetRoute -PolicyStore ActiveStore -ErrorAction Stop)
}
catch {
    throw ('Cannot read the IP configuration: {0}' -f $_.Exception.Message)
}
$addressesByIndex = @{}
foreach ($address in $addresses) {
    $key = [string](Get-Number $address.InterfaceIndex)
    if (-not $addressesByIndex.ContainsKey($key)) {
        $addressesByIndex[$key] = New-Object System.Collections.Generic.List[object]
    }
    $addressesByIndex[$key].Add($address)
}
# Default routes, lowest metric first.
$v4Routes = @{}
$v6Routes = @{}
foreach ($route in @($routes | Sort-Object -Property @{ Expression = { Get-Number $_.RouteMetric } })) {
    $key = [string](Get-Number $route.InterfaceIndex)
    $prefix = Get-Text $route.DestinationPrefix
    if ($prefix -eq '0.0.0.0/0') {
        if (-not $v4Routes.ContainsKey($key)) {
            $v4Routes[$key] = New-Object System.Collections.Generic.List[string]
        }
        $v4Routes[$key].Add((Get-Text $route.NextHop))
    }
    elseif ($prefix -eq '::/0') {
        $v6Routes[$key] = $true
    }
}

$chains = New-Object System.Collections.Generic.List[object]
foreach ($info in $infos) {
    if (($info.State -ne 'up') -or ($info.Kind -eq 'bluetooth')) {
        continue
    }
    $chain = Get-InterfaceChain $info
    # Virtual adapters only count when they carry the way to a real router.
    if ((-not $info.Physical) -and ($chain.Gateway.Length -eq 0)) {
        continue
    }
    $chains.Add($chain)
}
# Windows' own dial-up (PPPoE "broadband connection") and VPN connections are
# not adapters in Get-NetAdapter. One that carries an IPv4 default route and has
# an address counts too; its "router" is the other end of the line (on-link).
$knownIndexes = @{}
foreach ($info in $infos) {
    $knownIndexes[[string]$info.Index] = $true
}
foreach ($key in @($v4Routes.Keys)) {
    if ($knownIndexes.ContainsKey($key)) {
        continue
    }
    $lineInfo = [pscustomobject]@{
        Index       = [long]$key
        Guid        = ''
        Description = 'Dial-up or VPN connection'
        Kind        = 'dial-up'
        State       = 'up'
        Physical    = $false
        LowPower    = $false
    }
    $chain = Get-InterfaceChain $lineInfo
    if ($chain.V4Stage -eq 2) {
        $chains.Add($chain)
    }
}
$best = $null
foreach ($chain in $chains) {
    if (($null -eq $best) -or ((Get-ChainScore $chain) -gt (Get-ChainScore $best))) {
        $best = $chain
    }
}

$facts['adapter'] = $best.Info.Description
$facts['adapter_kind'] = $best.Info.Kind
$facts['apipa'] = $best.Apipa
if ($best.Conflict) {
    $facts['address_conflict'] = $true
}
$facts['manual_address'] = $best.Manual
$facts['has_gateway'] = ($best.V4Stage -eq 2)
$bestProfile = $profileByIndex[[string]$best.Info.Index]
if ($null -ne $bestProfile) {
    $facts['ipv4_connectivity'] = $bestProfile.V4
    $facts['ipv6_connectivity'] = $bestProfile.V6
}
else {
    $facts['ipv4_connectivity'] = 'none'
    $facts['ipv6_connectivity'] = 'none'
}

# ---- 5. The router and the way beyond it, tested at the same time.
$probe = $null
$probeNames = $null
if (@($chains | Where-Object { ($_.V4Stage -eq 2) -or $_.V6Route }).Count -gt 0) {
    $probeNames = Select-DnsProbeName
    $probe = Start-DnsProbe -Names $probeNames.Names
}

$pingOk = $false
$arpOk = $false
if (($best.V4Stage -eq 2) -and ($best.Gateway.Length -gt 0)) {
    $pingOk = Test-GatewayPing -Address $best.Gateway
    $facts['gateway_ping'] = $pingOk
    if (-not $pingOk) {
        $arpOk = Test-GatewayNeighbor -Address $best.Gateway -Index $best.Info.Index
        $facts['gateway_arp'] = $arpOk
    }
}

$dns = 'unknown'
if ($null -ne $probe) {
    $answers = @(Receive-DnsProbe -Probe $probe)
    $errors = @($answers | ForEach-Object { $_.Error })
    if (@($answers | Where-Object { $_.Resolved }).Count -gt 0) {
        $dns = 'ok'
    }
    elseif ($errors -contains 'no-servers') {
        $dns = 'no-servers'
    }
    elseif (@($errors | Where-Object { ($_ -ne 'not-run') -and ($_ -ne 'unavailable') }).Count -gt 0) {
        $dns = 'failed'
    }
    $facts['dns'] = $dns
}

$upGuids = @($chains | ForEach-Object { $_.Info.Guid })
$manualDns = Test-ManualDnsServer -Guids $upGuids
$facts['manual_dns'] = $manualDns
$probeOff = Test-NcsiProbeOff
$facts['ncsi_probe_off'] = $probeOff

if ($dns -eq 'ok') {
    if ($probeOff) {
        Write-Result 'ncsi-off'
    }
    else {
        Write-Result 'internet-unconfirmed'
    }
}
elseif ($best.V4Stage -eq 0) {
    Write-Result 'no-address'
}
elseif ($best.V4Stage -eq 1) {
    if ($best.Manual) {
        Write-Result 'manual-address'
    }
    else {
        Write-Result 'no-gateway'
    }
}
else {
    $routerAnswered = ($pingOk -or $arpOk -or $best.OnLink)
    $facts['gateway_reachable'] = $routerAnswered
    if (-not $routerAnswered) {
        if ($best.Manual) {
            Write-Result 'manual-address'
        }
        else {
            Write-Result 'gateway-unreachable'
        }
    }
    elseif ($manualDns -or ($dns -eq 'no-servers')) {
        Write-Result 'dns-suspect'
    }
    elseif ($dns -eq 'unknown') {
        Write-Result 'dns-untested'
    }
    else {
        Write-Result 'no-internet'
    }
}
