# Check: hardware.battery-health
# Laptop battery wear: full-charge capacity compared with design capacity.
#   No Win32_Battery instance -> no-battery (desktop PC; status na).
#   Otherwise: powercfg /batteryreport /xml /output <temp file>, read
#   Batteries/Battery/{DesignCapacity, FullChargeCapacity, CycleCount} (mWh),
#   then delete the temp file. Several batteries are added up.
# The report XML uses a default namespace, so elements are matched by local name.
# Read-only. Result codes: good (>= 80 %) / worn (50-80 %) / poor (< 50 %) / no-battery.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

function Get-ChildText {
    param($Node, [string]$LocalName)
    foreach ($child in $Node.ChildNodes) {
        if ($child.LocalName -eq $LocalName) {
            return ([string]$child.InnerText).Trim()
        }
    }
    return ''
}

function Read-XmlFile {
    param([string]$Path)
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $reader = [System.Xml.XmlReader]::Create($Path, $settings)
    try {
        $doc = New-Object System.Xml.XmlDocument
        $doc.Load($reader)
        # The unary comma keeps the pipeline from enumerating the document's nodes.
        return , $doc
    }
    finally {
        $reader.Close()
    }
}

$batteries = @(Get-CimInstance -ClassName Win32_Battery)
if ($batteries.Count -eq 0) {
    [pscustomobject]@{
        result = 'no-battery'
        facts  = [ordered]@{ battery_count = 0 }
    }
    return
}

# 32-bit PowerShell on 64-bit Windows sees SysWOW64 through "System32".
$systemDir = Join-Path $env:windir 'System32'
if ([Environment]::Is64BitOperatingSystem -and (-not [Environment]::Is64BitProcess)) {
    $systemDir = Join-Path $env:windir 'Sysnative'
}
$powercfg = Join-Path $systemDir 'powercfg.exe'
$reportPath = Join-Path ([System.IO.Path]::GetTempPath()) ('medkit-battery-{0}.xml' -f [guid]::NewGuid().ToString('N'))

$doc = $null
try {
    # powercfg prints a localized "report saved" line; it is discarded. With
    # ErrorActionPreference 'Stop', Windows PowerShell 5.1 would turn any stderr
    # line of a native command into a terminating error, so relax it here and
    # judge success by the exit code and the output file instead.
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $null = & $powercfg '/batteryreport' '/xml' '/output' $reportPath 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
        throw ('powercfg did not write the battery report (exit code {0})' -f $exitCode)
    }
    $doc = Read-XmlFile $reportPath
}
finally {
    if (Test-Path -LiteralPath $reportPath) {
        Remove-Item -LiteralPath $reportPath -Force -ErrorAction SilentlyContinue
    }
}

$nodes = @($doc.SelectNodes("//*[local-name()='Batteries']/*[local-name()='Battery']"))
if ($nodes.Count -eq 0) {
    throw 'The battery report lists no batteries'
}

$design = [double]0
$full = [double]0
$cycles = [long]0
foreach ($n in $nodes) {
    $value = [long]0
    if ([long]::TryParse((Get-ChildText $n 'DesignCapacity'), [ref]$value)) {
        $design += $value
    }
    $value = [long]0
    if ([long]::TryParse((Get-ChildText $n 'FullChargeCapacity'), [ref]$value)) {
        $full += $value
    }
    $value = [long]0
    if ([long]::TryParse((Get-ChildText $n 'CycleCount'), [ref]$value)) {
        if ($value -gt $cycles) {
            $cycles = $value
        }
    }
}

if ($design -le 0) {
    throw 'The battery does not report its design capacity'
}
if ($full -le 0) {
    throw 'The battery does not report its full charge capacity'
}

$healthPct = $full / $design * 100

$result = 'good'
if ($healthPct -lt 50) {
    $result = 'poor'
}
elseif ($healthPct -lt 80) {
    $result = 'worn'
}

# New batteries can report a little more than their design capacity.
$shownPct = [math]::Min([math]::Round($healthPct, 1), 100)

$facts = [ordered]@{
    battery_count = $nodes.Count
    design_mwh    = [long]$design
    full_mwh      = [long]$full
    health_pct    = $shownPct
}
if ($cycles -gt 0) {
    $facts['cycle_count'] = $cycles
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
