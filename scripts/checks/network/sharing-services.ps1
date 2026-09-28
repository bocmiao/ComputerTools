# Check: network.sharing-services
# Are the services shared folders need switched off? "Optimizer" tools do
# it, and in 2017 (WannaCry) many guides told people to disable Server to
# close port 445. Windows defaults (Microsoft, "Guidance on disabling system
# services on Windows Server"; SMBv1 article for the last two):
# - Workstation (LanmanWorkstation): Automatic. Without it this computer
#   cannot open any shared folder: "network path not found" (0x80070035).
# - Server (LanmanServer): Automatic, "Do not disable". Without it nobody can
#   open the folders this computer shares.
# - Function Discovery Resource Publication (FDResPub) and Function Discovery
#   Provider Host (fdPHost): Manual. They make this computer show up in the
#   Network folder of the others, and find them. Since SMB1 and the Computer
#   Browser are gone, Microsoft's advice for home networks is to start both
#   and set them to Automatic (Delayed Start).
# - TCP/IP NetBIOS Helper (lmhosts): Manual. Finding computers by name on
#   small networks without a DNS server.
# Read-only. Result codes, in this order: missing (one is not there) /
# client-disabled / server-disabled / discovery-disabled (FDResPub or
# fdPHost) / netbios-disabled / not-announced (FDResPub or fdPHost is set to
# Manual and not running: the others do not see this computer) / ok. All but
# missing are fixed by network.enable-sharing-services.
# Facts: workstation, server, fdrespub, fdphost, lmhosts (Start values, ''
# when missing), announcing (both discovery services running or set to
# start by themselves).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# A service's Start value, or $null when the service is not there.
function Get-StartValue {
    param([string]$Name)
    try {
        return [int](Get-ItemProperty -LiteralPath ('HKLM:\SYSTEM\CurrentControlSet\Services\' + $Name) -Name 'Start' -ErrorAction Stop).Start
    }
    catch {
        return $null
    }
}

# $true when the service is running; $false when it is not or cannot be read.
function Test-Running {
    param([string]$Name)
    try {
        return ([string](Get-Service -Name $Name -ErrorAction Stop).Status -eq 'Running')
    }
    catch {
        return $false
    }
}

$names = [ordered]@{
    workstation = 'LanmanWorkstation'
    server      = 'LanmanServer'
    fdrespub    = 'FDResPub'
    fdphost     = 'fdPHost'
    lmhosts     = 'lmhosts'
}
$start = @{}
$facts = [ordered]@{}
$missing = $false
foreach ($fact in $names.Keys) {
    $value = Get-StartValue $names[$fact]
    $start[$fact] = $value
    $facts[$fact] = ''
    if ($null -eq $value) {
        $missing = $true
    }
    else {
        $facts[$fact] = $value
    }
}

# A discovery service announces this computer when it runs now or starts by
# itself (Automatic, 2); Manual (3) ones start only on demand.
$announcing = $true
foreach ($fact in @('fdrespub', 'fdphost')) {
    if (($start[$fact] -ne 2) -and (-not (Test-Running $names[$fact]))) {
        $announcing = $false
    }
}
$facts['announcing'] = $announcing

$result = 'ok'
if ($missing) {
    $result = 'missing'
}
elseif ($start.workstation -eq 4) {
    $result = 'client-disabled'
}
elseif ($start.server -eq 4) {
    $result = 'server-disabled'
}
elseif (($start.fdrespub -eq 4) -or ($start.fdphost -eq 4)) {
    $result = 'discovery-disabled'
}
elseif ($start.lmhosts -eq 4) {
    $result = 'netbios-disabled'
}
elseif (-not $announcing) {
    $result = 'not-announced'
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
