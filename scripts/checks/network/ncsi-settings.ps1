# Check: network.ncsi-settings
# Has Windows' own internet test (Network Connectivity Status Indicator,
# NCSI) been switched off or pointed somewhere else? Then the taskbar can say
# "No internet" while web pages open, and Office, Microsoft Store and other
# apps that ask NCSI think they are offline. "Optimizer" tools and old guides
# ("fix No internet") change it; Microsoft says not to disable active probing
# to resolve an issue.
# The defaults in Windows 10 1607 and later, under
# HKLM\SYSTEM\CurrentControlSet\Services\NlaSvc\Parameters\Internet
# (Microsoft, "Network Connection Status Indicator (NCSI) troubleshooting
# guidance"): EnableActiveProbing 1 (DWORD) and the REG_SZ values in
# $defaults below. network.ncsi-defaults writes exactly these.
# The Group Policy "Turn off Windows Network Connectivity Status Indicator
# active tests" (NoActiveProbe = 1 under
# HKLM\SOFTWARE\Policies\Microsoft\Windows\NetworkConnectivityStatusIndicator)
# is only reported: policies are left to whoever set them.
# Read-only. Result codes, in this order: policy / probe-off
# (EnableActiveProbing is 0) / changed (a probe value is different or
# missing) / ok. missing when the Internet key itself is not there.
# Facts: probing (EnableActiveProbing, '' when missing), changed (the names
# of the values that differ, comma separated; their data is not output, it
# may name a company's own server), policy (true / false).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$internetKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\NlaSvc\Parameters\Internet'
$policyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\NetworkConnectivityStatusIndicator'
$defaults = [ordered]@{
    ActiveWebProbeHost      = 'www.msftconnecttest.com'
    ActiveWebProbePath      = 'connecttest.txt'
    ActiveWebProbeContent   = 'Microsoft Connect Test'
    ActiveDnsProbeHost      = 'dns.msftncsi.com'
    ActiveDnsProbeContent   = '131.107.255.255'
    ActiveWebProbeHostV6    = 'ipv6.msftconnecttest.com'
    ActiveWebProbePathV6    = 'connecttest.txt'
    ActiveWebProbeContentV6 = 'Microsoft Connect Test'
    ActiveDnsProbeHostV6    = 'dns.msftncsi.com'
    ActiveDnsProbeContentV6 = 'fd3e:4f5a:5b81::1'
}

# The values of a key as a PSObject, or $null when the key cannot be read.
function Get-KeyValues {
    param([string]$Path)
    try {
        return (Get-ItemProperty -LiteralPath $Path -ErrorAction Stop)
    }
    catch {
        return $null
    }
}

function Get-Value {
    param($Values, [string]$Name)
    if ($null -eq $Values) {
        return $null
    }
    $prop = $Values.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    return $prop.Value
}

$values = Get-KeyValues $internetKey
$policy = Get-KeyValues $policyKey
$facts = [ordered]@{ probing = ''; changed = ''; policy = $false }

$noActiveProbe = Get-Value $policy 'NoActiveProbe'
if (($null -ne $noActiveProbe) -and ([string]$noActiveProbe -eq '1')) {
    $facts.policy = $true
}

$result = 'missing'
if ($null -ne $values) {
    $probing = Get-Value $values 'EnableActiveProbing'
    if ($null -ne $probing) {
        $facts.probing = $probing
    }
    $changed = New-Object System.Collections.Generic.List[string]
    # 0 is probe-off below; missing or anything else but 1 is a change too.
    if (($null -eq $probing) -or (([string]$probing -ne '1') -and ([string]$probing -ne '0'))) {
        $changed.Add('EnableActiveProbing')
    }
    foreach ($name in $defaults.Keys) {
        $now = Get-Value $values $name
        # Host names and IP addresses are not case sensitive; the expected
        # page content is compared the same way, NCSI itself is lenient.
        if (($null -eq $now) -or (-not ([string]$now).Trim().Equals($defaults[$name], [StringComparison]::OrdinalIgnoreCase))) {
            $changed.Add($name)
        }
    }
    $facts.changed = $changed -join ', '
    if ($facts.policy) {
        $result = 'policy'
    }
    elseif (($null -ne $probing) -and ([string]$probing -eq '0')) {
        $result = 'probe-off'
    }
    elseif ($changed.Count -gt 0) {
        $result = 'changed'
    }
    else {
        $result = 'ok'
    }
}
elseif ($facts.policy) {
    $result = 'policy'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
