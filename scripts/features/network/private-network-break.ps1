# Feature: network.private-network -- break (tests only)
# Sets every connected Private network (on a physical adapter) to Public.
# Nothing to do when they are Public already; throws when nothing is
# connected.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block network-profile: identical in checks/network/network-category.ps1 and features/network/private-network-*.ps1 (medkit-data check compares them) ----
# The networks this computer is connected to through a physical adapter
# (Get-NetConnectionProfile; Get-NetAdapter HardwareInterface or
# ConnectorPresent, not EndPointInterface: VPNs and the virtual switches of
# Hyper-V and WSL, often an "Unidentified network" set to Public, are left
# out). NetworkCategory: Public 0, Private 1, DomainAuthenticated 2
# (MSFT_NetConnectionProfile), as a number or as its name.
# Windows keeps the category of every network it has seen under
# HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\NetworkList\Profiles\{GUID}
# (ProfileName, Category 0 / 1 / 2): Set-NetConnectionProfile changes a
# connected one, the Category value one that is not connected now (it is
# used the next time it connects). Profiles are recorded by that GUID, never
# by name: network names are WiFi names and stay out of logs and reports.
$networkProfilesKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\NetworkList\Profiles'

function Get-NetworkText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

function Test-NetworkTrue {
    param($Value)
    if ($null -eq $Value) {
        return $false
    }
    if ($Value -is [bool]) {
        return $Value
    }
    $t = (Get-NetworkText $Value).ToLowerInvariant()
    return (($t -eq 'true') -or ($t -eq '1'))
}

# public / private / domain / '' from a NetworkCategory or a Category value.
function Get-CategoryName {
    param($Value)
    switch ((Get-NetworkText $Value).ToLowerInvariant()) {
        '0' { return 'public' }
        'public' { return 'public' }
        '1' { return 'private' }
        'private' { return 'private' }
        '2' { return 'domain' }
        'domainauthenticated' { return 'domain' }
        default { return '' }
    }
}

# The GUID (with braces, lower case) of the saved network named $Name, or ''
# when there is none or more than one.
function Find-NetworkProfileGuid {
    param([string]$Name)
    $found = @()
    foreach ($key in @(Get-ChildItem -LiteralPath $networkProfilesKey -ErrorAction SilentlyContinue)) {
        $item = $null
        try {
            $item = Get-ItemProperty -LiteralPath $key.PSPath -ErrorAction Stop
        }
        catch {
            continue
        }
        if ((Get-NetworkText $item.ProfileName) -eq $Name) {
            $found += ([string]$key.PSChildName).ToLowerInvariant()
        }
    }
    if ($found.Count -eq 1) {
        return $found[0]
    }
    return ''
}

# The networks connected through physical adapters, as @{ Index (the
# interface index); Name (only for matching, never output); Category;
# Guid (of the saved network, '' when not found) }, or $null when
# Get-NetConnectionProfile / Get-NetAdapter do not work here.
function Get-ConnectedNetworks {
    $adapters = $null
    try {
        $adapters = @(Get-NetAdapter -ErrorAction Stop)
    }
    catch {
        return $null
    }
    # Nothing connected: no profiles (some Windows versions report that as
    # an error).
    $profiles = @()
    try {
        $profiles = @(Get-NetConnectionProfile -ErrorAction Stop)
    }
    catch {
        $profiles = @()
    }
    $physical = @{}
    foreach ($adapter in $adapters) {
        if (($null -eq $adapter) -or (Test-NetworkTrue $adapter.EndPointInterface) -or (-not ((Test-NetworkTrue $adapter.HardwareInterface) -or (Test-NetworkTrue $adapter.ConnectorPresent)))) {
            continue
        }
        $index = $adapter.InterfaceIndex
        if ($null -eq $index) {
            $index = $adapter.ifIndex
        }
        if ($null -ne $index) {
            $physical[[string]$index] = $true
        }
    }
    $list = New-Object System.Collections.Generic.List[object]
    foreach ($connection in $profiles) {
        if (($null -eq $connection) -or ($null -eq $connection.InterfaceIndex) -or (-not $physical.ContainsKey([string]$connection.InterfaceIndex))) {
            continue
        }
        $name = Get-NetworkText $connection.Name
        $list.Add([pscustomobject]@{
                Index    = [int]$connection.InterfaceIndex
                Name     = $name
                Category = (Get-CategoryName $connection.NetworkCategory)
                Guid     = (Find-NetworkProfileGuid $name)
            })
    }
    return , $list.ToArray()
}

# Waits (up to 10 seconds) until the connected network on interface $Index
# reads back as $Category; $true when it does.
function Wait-NetworkCategory {
    param([int]$Index, [string]$Category)
    $deadline = (Get-Date).AddSeconds(10)
    while ($true) {
        $now = @(Get-ConnectedNetworks | Where-Object { ($null -ne $_) -and ($_.Index -eq $Index) })
        if (($now.Count -gt 0) -and ($now[0].Category -eq $Category)) {
            return $true
        }
        if ((Get-Date) -ge $deadline) {
            return $false
        }
        Start-Sleep -Milliseconds 500
    }
}

# ---- end of shared block network-profile ----

$networks = Get-ConnectedNetworks
if (($null -eq $networks) -or ($networks.Count -eq 0)) {
    throw 'no connected network'
}
foreach ($network in @($networks | Where-Object { $_.Category -eq 'private' })) {
    Set-NetConnectionProfile -InterfaceIndex $network.Index -NetworkCategory Public -ErrorAction Stop
    if (-not (Wait-NetworkCategory -Index $network.Index -Category 'public')) {
        throw 'the network is still Private after Set-NetConnectionProfile'
    }
}

[pscustomobject]@{ result = 'ok' }
