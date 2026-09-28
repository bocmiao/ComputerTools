# Check: network.sharing-firewall
# Does Windows Firewall let sharing through on private networks? Read-only:
# medkit never changes firewall settings, the user turns them on in
# "Advanced sharing settings" (network discovery, file and printer sharing
# for private networks turn on exactly these rule groups).
# - Rule groups are found by their resource names, the same in every
#   language: File and Printer Sharing '@FirewallAPI.dll,-28502', Network
#   Discovery '@FirewallAPI.dll,-32752'. A group is on for private networks
#   when one of its enabled inbound Allow rules covers the Private profile
#   (Profile Any / Private, or the number 0 / bit 2).
# - Blocked: an enabled inbound Block rule for TCP port 445 or 139 (SMB and
#   NetBIOS sessions) covering the Private profile. Block beats Allow. Many
#   such rules come from the 2017 WannaCry guides ("close port 445") or from
#   security software. Rules for any port are not counted.
# - The Private firewall profile switched off: nothing blocks sharing there
#   (security.firewall reports the firewall itself).
# Result codes, in this order: blocked / sharing-off / discovery-off / ok /
# unreadable (the firewall cannot be read, e.g. its service is stopped).
# Facts: firewall_private (true / false / ''), sharing (on / off), discovery
# (on / off), blocked_count, rule (the display name of the first blocking
# rule, so that it can be found; rule names are set by software and guides).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$sharingGroup = '@FirewallAPI.dll,-28502'
$discoveryGroup = '@FirewallAPI.dll,-32752'
$sharingPorts = @(445, 139)

function Get-Text {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([string]$Value).Trim()
}

# True when a rule's Profile (a flags enum, as its name or number) covers the
# Private profile.
function Test-CoversPrivate {
    param($ProfileValue)
    $t = Get-Text $ProfileValue
    $n = 0
    if ([int]::TryParse($t, [ref]$n)) {
        return (($n -eq 0) -or (($n -band 2) -ne 0))
    }
    return ($t -match '(?i)\b(Any|Private)\b')
}

function Test-Enabled {
    param($Value)
    $t = (Get-Text $Value).ToLowerInvariant()
    return (($t -eq 'true') -or ($t -eq '1'))
}

# True when a LocalPort value (a port, a range "a-b", or a list of these)
# includes one of the sharing ports; 'Any' does not count.
function Test-SharingPort {
    param($LocalPort)
    foreach ($item in @($LocalPort)) {
        foreach ($part in ((Get-Text $item) -split ',')) {
            $p = $part.Trim()
            $low = 0
            $high = 0
            if ($p -match '^(\d+)-(\d+)$') {
                $low = [int]$Matches[1]
                $high = [int]$Matches[2]
            }
            elseif ($p -match '^\d+$') {
                $low = [int]$p
                $high = $low
            }
            else {
                continue
            }
            foreach ($port in $sharingPorts) {
                if (($port -ge $low) -and ($port -le $high)) {
                    return $true
                }
            }
        }
    }
    return $false
}

# 'on' when an enabled inbound Allow rule of the group covers private networks.
function Get-GroupState {
    param([string]$Group)
    foreach ($rule in @(Get-NetFirewallRule -Group $Group -ErrorAction Stop)) {
        if (($null -ne $rule) -and (Test-Enabled $rule.Enabled) -and ((Get-Text $rule.Direction) -match '(?i)^(Inbound|1)$') -and ((Get-Text $rule.Action) -match '(?i)^(Allow|2)$') -and (Test-CoversPrivate $rule.Profile)) {
            return 'on'
        }
    }
    return 'off'
}

$facts = [ordered]@{ firewall_private = ''; sharing = ''; discovery = ''; blocked_count = 0; rule = '' }
$result = 'ok'
try {
    $privateProfile = Get-NetFirewallProfile -Name Private -ErrorAction Stop
    $facts.firewall_private = Test-Enabled $privateProfile.Enabled
    $facts.sharing = Get-GroupState $sharingGroup
    $facts.discovery = Get-GroupState $discoveryGroup
    $blocking = New-Object System.Collections.Generic.List[string]
    foreach ($rule in @(Get-NetFirewallRule -Direction Inbound -Action Block -Enabled True -ErrorAction Stop)) {
        if (($null -eq $rule) -or (-not (Test-CoversPrivate $rule.Profile))) {
            continue
        }
        $filter = Get-NetFirewallPortFilter -AssociatedNetFirewallRule $rule -ErrorAction Stop
        if (((Get-Text $filter.Protocol) -match '(?i)^(TCP|6|Any)$') -and (Test-SharingPort $filter.LocalPort)) {
            $name = Get-Text $rule.DisplayName
            if ($name.Length -eq 0) {
                $name = Get-Text $rule.Name
            }
            $blocking.Add($name)
        }
    }
    $facts.blocked_count = $blocking.Count
    if ($blocking.Count -gt 0) {
        $facts.rule = $blocking[0]
    }
    if (-not $facts.firewall_private) {
        $result = 'ok'
    }
    elseif ($blocking.Count -gt 0) {
        $result = 'blocked'
    }
    elseif ($facts.sharing -eq 'off') {
        $result = 'sharing-off'
    }
    elseif ($facts.discovery -eq 'off') {
        $result = 'discovery-off'
    }
}
catch {
    $result = 'unreadable'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
