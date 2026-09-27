# Check: security.firewall
# Is Windows Firewall on for the network types a home PC uses (Private and
# Public)? The Domain profile only matters on a company network and is not
# judged. Get-NetFirewallProfile -PolicyStore ActiveStore gives the state in
# force (settings and policies together).
# A firewall of another security product takes over when it is on: Windows
# then turns its own firewall off by design. Such products register in
# root\SecurityCenter2, FirewallProduct; productState holds the state in
# 0xF000 (0x1000 = on) and the owner in 0x0F00 (0 = not Microsoft). The
# namespace does not exist on Windows Server.
# Medkit never turns the firewall on or off itself (plan, section 5): the
# user does it in the firewall settings.
# Result codes: ok / third-party (another product's firewall is on) / off
# (Private or Public is off and no other firewall is on) / unknown-state (the
# profiles cannot be read, e.g. the firewall service is disabled).
# Facts: private, public (true / false), products (the names of the other
# firewalls that are on, comma separated).
# Read-only.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$profiles = @{}
try {
    foreach ($p in @(Get-NetFirewallProfile -PolicyStore ActiveStore -ErrorAction Stop)) {
        $profiles[[string]$p.Name] = ([string]$p.Enabled -eq 'True')
    }
}
catch {
    $profiles = @{}
}
if ((-not $profiles.ContainsKey('Private')) -or (-not $profiles.ContainsKey('Public'))) {
    [pscustomobject]@{ result = 'unknown-state'; facts = [ordered]@{ products = '' } }
    return
}

$products = New-Object System.Collections.Generic.List[string]
try {
    foreach ($product in @(Get-CimInstance -Namespace 'root/SecurityCenter2' -ClassName 'FirewallProduct' -ErrorAction Stop)) {
        $state = [int64]0
        if (-not [int64]::TryParse([string]$product.productState, [ref]$state)) {
            continue
        }
        $on = ($state -band 0xF000) -eq 0x1000
        $microsoft = ($state -band 0x0F00) -ne 0
        $name = ([string]$product.displayName).Trim()
        if ($on -and (-not $microsoft) -and ($name.Length -gt 0) -and -not $products.Contains($name)) {
            $products.Add($name)
        }
    }
}
catch {
    # No Security Center (Windows Server) or it does not answer: no other firewall known.
    $products.Clear()
}

$result = 'ok'
if ((-not $profiles['Private']) -or (-not $profiles['Public'])) {
    $result = 'off'
    if ($products.Count -gt 0) {
        $result = 'third-party'
    }
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        private  = $profiles['Private']
        public   = $profiles['Public']
        products = ($products.ToArray() -join ', ')
    }
}
