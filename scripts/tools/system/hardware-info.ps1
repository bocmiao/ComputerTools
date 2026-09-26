# Tool: system.hardware-info
# The main parts of this PC, as tables for the "info" tool view
# (docs/architecture.md 11.2). Read-only.
#
# Sections, in this order (ids and codes are labelled in the YAML):
#   computer  maker, model (Win32_ComputerSystem). Lenovo keeps the friendly
#             model name in Win32_ComputerSystemProduct.Version and the machine
#             type in Model; both are shown when they differ.
#   os        name (Win32_OperatingSystem.Caption, already localized), version
#             (DisplayVersion or ReleaseId plus CurrentBuild.UBR from the
#             registry), arch (native PROCESSOR_ARCHITECTURE: x64 / arm64 / x86),
#             install_date, activation (activated / not-activated / unknown).
#   cpu       one section per processor: cores, threads, clock (MaxClockSpeed).
#   board     maker, model (Win32_BaseBoard), bios_version, bios_date (Win32_BIOS).
#   memory    total, slots (used / total, only when the firmware reports slots),
#             one "module" row per memory module: size, type, speed, maker.
#   gpu       one section per video controller: driver_status (only for the
#             Microsoft Basic Display Adapter, i.e. no graphics driver), vram,
#             driver_version, driver_date, resolution.
#   disk      one section per physical disk: size, type (ssd / hdd / unknown,
#             from MediaType only), bus.
#   network   one section per physical network adapter: kind (wifi / ethernet /
#             other), status (up / down / disabled).
#
# Privacy: the tool view has a "copy all" button and people send the result to
# others, so serial numbers, UUIDs, MAC and IP addresses, the computer name, user
# names and product keys are never read into the output. The activation query
# selects LicenseStatus only.
#
# Robustness: every part is read on its own; a part that fails is left out and
# the result becomes "partial". The script only throws when nothing at all could
# be read. Firmware placeholders ("System manufacturer", "To be filled by
# O.E.M.", "Default string", ...) are left out.
#
# Video memory: Win32_VideoController.AdapterRAM is a uint32 and saturates at
# 4 GB. The display driver writes the real size to its software key
# (Control\Class\{4d36e968-...}\NNNN, found through the Driver value of the
# device's Enum key): HardwareInformation.qwMemorySize (QWORD, bytes) when
# present, else HardwareInformation.MemorySize (documented as megabytes, written
# as bytes by most drivers: values below 1 MB are taken as megabytes). A 32-bit
# value that is saturated is not shown.
#
# Result codes: ok / partial. Facts: section_count, failed_count.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$invariant = [System.Globalization.CultureInfo]::InvariantCulture

# Text that firmware vendors leave in SMBIOS fields nobody filled in.
$placeholders = @(
    'system manufacturer', 'system product name', 'system version', 'system sku',
    'to be filled by o.e.m.', 'to be filled by oem', 'default string', 'default',
    'not applicable', 'not specified', 'not available', 'none', 'n/a', 'na',
    'oem', 'o.e.m.', 'undefined', 'unknown', 'invalid', 'empty',
    'base board manufacturer', 'base board product name', 'type1productconfigid',
    'x.x', '1234567890', '0123456789'
)

# SMBIOS type 17 memory types (DSP0134).
$memoryTypes = @{
    18 = 'DDR'; 19 = 'DDR2'; 20 = 'DDR2'; 24 = 'DDR3'; 26 = 'DDR4'
    27 = 'LPDDR'; 28 = 'LPDDR2'; 29 = 'LPDDR3'; 30 = 'LPDDR4'; 34 = 'DDR5'; 35 = 'LPDDR5'
}

# MSFT_PhysicalDisk value maps; Get-PhysicalDisk usually returns the names.
$mediaNames = @{ '0' = 'Unspecified'; '3' = 'HDD'; '4' = 'SSD'; '5' = 'SCM' }
$busNames = @{
    '0' = 'Unknown'; '1' = 'SCSI'; '2' = 'ATAPI'; '3' = 'ATA'; '4' = '1394'; '5' = 'SSA'
    '6' = 'Fibre Channel'; '7' = 'USB'; '8' = 'RAID'; '9' = 'iSCSI'; '10' = 'SAS'; '11' = 'SATA'
    '12' = 'SD'; '13' = 'MMC'; '14' = 'Virtual'; '15' = 'File Backed Virtual'
    '16' = 'Storage Spaces'; '17' = 'NVMe'; '18' = 'SCM'; '19' = 'UFS'
}

$displayClass = '{4d36e968-e325-11ce-bfc1-08002be10318}'
# A 32-bit size at or above this value may be capped at 4 GB.
$saturated = [double]4293918720

$sections = New-Object System.Collections.Generic.List[object]
$failed = New-Object System.Collections.Generic.List[string]

function Add-Failure {
    param([string]$Part)
    if (-not $failed.Contains($Part)) {
        $failed.Add($Part)
    }
}

# Trimmed text with runs of white space and control characters collapsed; ''
# for null.
function Get-CleanText {
    param($Value)
    if ($null -eq $Value) {
        return ''
    }
    return ([regex]::Replace([string]$Value, '[\s\x00-\x1F\x7F]+', ' ')).Trim()
}

# False for empty text and firmware placeholders.
function Test-RealValue {
    param([string]$Text)
    if ($Text.Length -eq 0) {
        return $false
    }
    if ($placeholders -contains $Text) {
        return $false
    }
    # Only zeros, dots, dashes or blanks.
    if ($Text -match '^[0.\- ]+$') {
        return $false
    }
    return $true
}

# A memory module maker only when it is a readable name: JEDEC ID codes
# ("80CE", "802C000080AD"), fillers ("Manufacturer00") and garbage are dropped.
function Get-ReadableMaker {
    param([string]$Text)
    if (-not (Test-RealValue $Text)) {
        return ''
    }
    if ($Text -match '^(0x)?[0-9A-Fa-f ]+$') {
        return ''
    }
    if ($Text -match '[^\x20-\x7E]') {
        return ''
    }
    if ($Text -match '(?i)^(manufacturer|module ?manufacturer|dimm|bank|array|slot)[ _-]?\d*$') {
        return ''
    }
    if ($Text -notmatch '[A-Za-z]{2,}') {
        return ''
    }
    return $Text
}

# Raw registry value, or $null when the key or the value is missing or cannot
# be read. The unary comma keeps a REG_BINARY byte array in one piece.
function Get-RegistryValue {
    param([string]$Path, [string]$Name)
    try {
        $key = Get-Item -LiteralPath $Path
    }
    catch {
        return $null
    }
    try {
        return , $key.GetValue($Name)
    }
    catch {
        return $null
    }
    finally {
        $key.Close()
    }
}

# Size from a registry or CIM number: QWORD, DWORD (negative above 2 GB),
# uint32 or REG_BINARY. 0 when unknown.
function ConvertTo-Bytes {
    param($Value)
    if ($null -eq $Value) {
        return [double]0
    }
    if ($Value -is [byte[]]) {
        if ($Value.Length -ge 8) {
            return [double][System.BitConverter]::ToUInt64($Value, 0)
        }
        if ($Value.Length -ge 4) {
            return [double][System.BitConverter]::ToUInt32($Value, 0)
        }
        return [double]0
    }
    $number = [double]0
    if (-not [double]::TryParse([string]$Value, [System.Globalization.NumberStyles]::Integer, $invariant, [ref]$number)) {
        return [double]0
    }
    if (($Value -is [int]) -and ($number -lt 0)) {
        $number += 4294967296
    }
    if ($number -lt 0) {
        return [double]0
    }
    return $number
}

# "16 GB", "15.8 GB", "512 MB".
function Format-MemorySize {
    param([double]$Bytes)
    if ($Bytes -ge 1GB) {
        return ([math]::Round($Bytes / 1GB, 1)).ToString('0.#', $invariant) + ' GB'
    }
    return ([math]::Round($Bytes / 1MB)).ToString('0', $invariant) + ' MB'
}

# "477 GB" below 1 TB, "1.8 TB" above (binary units, as Windows shows them).
function Format-DiskSize {
    param([double]$Bytes)
    if ($Bytes -ge 1TB) {
        return ([math]::Round($Bytes / 1TB, 1)).ToString('0.#', $invariant) + ' TB'
    }
    return ([math]::Round($Bytes / 1GB)).ToString('0', $invariant) + ' GB'
}

# "1.8 GHz"; below 1 GHz "800 MHz".
function Format-Clock {
    param([double]$Mhz)
    if ($Mhz -ge 1000) {
        return ($Mhz / 1000).ToString('0.0', $invariant) + ' GHz'
    }
    return $Mhz.ToString('0', $invariant) + ' MHz'
}

function Format-Date {
    param($Value)
    if ($Value -is [datetime]) {
        return $Value.ToString('yyyy-MM-dd', $invariant)
    }
    return ''
}

# Positive integer from a CIM property, or 0.
function Get-Count {
    param($Value)
    $n = [long]0
    if (($null -ne $Value) -and [long]::TryParse([string]$Value, [ref]$n) -and ($n -gt 0)) {
        return $n
    }
    return [long]0
}

function ConvertTo-Name {
    param($Value, [hashtable]$Names)
    $text = [string]$Value
    if ($Names.ContainsKey($text)) {
        return $Names[$text]
    }
    return $text
}

function New-ValueRow {
    param([string]$Id, $Value)
    return [ordered]@{ id = $Id; value = $Value }
}

function New-CodeRow {
    param([string]$Id, [string]$Code)
    return [ordered]@{ id = $Id; code = $Code }
}

# Adds a section with at least one row. $Rows is a List[object]; ToArray()
# makes it a real array (@() on such a list fails in some PowerShell versions).
function Add-Section {
    param([string]$Id, [string]$Name, [System.Collections.Generic.List[object]]$Rows)
    $list = $Rows.ToArray()
    if ($list.Count -eq 0) {
        return
    }
    $section = [ordered]@{ id = $Id }
    if ($Name.Length -gt 0) {
        $section['name'] = $Name
    }
    $section['rows'] = $list
    $sections.Add($section)
}

# "24H2 (26100.4652)"; Windows 10 before 20H2 has ReleaseId instead of
# DisplayVersion.
function Get-WindowsVersion {
    $path = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    $display = Get-CleanText (Get-RegistryValue -Path $path -Name 'DisplayVersion')
    if ($display.Length -eq 0) {
        $display = Get-CleanText (Get-RegistryValue -Path $path -Name 'ReleaseId')
    }
    $build = Get-CleanText (Get-RegistryValue -Path $path -Name 'CurrentBuild')
    if ($build.Length -eq 0) {
        $build = Get-CleanText (Get-RegistryValue -Path $path -Name 'CurrentBuildNumber')
    }
    $ubr = Get-RegistryValue -Path $path -Name 'UBR'
    $ubrNumber = [long]0
    if (($build.Length -gt 0) -and ($null -ne $ubr) -and [long]::TryParse([string]$ubr, [ref]$ubrNumber) -and ($ubrNumber -ge 0)) {
        $build = '{0}.{1}' -f $build, $ubrNumber
    }
    if (($display.Length -gt 0) -and ($build.Length -gt 0)) {
        return ('{0} ({1})' -f $display, $build)
    }
    if ($display.Length -gt 0) {
        return $display
    }
    return $build
}

# Native architecture from the system environment (not the process, which may
# be 32-bit or emulated). '' when unknown.
function Get-ArchitectureCode {
    $native = Get-CleanText (Get-RegistryValue -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' -Name 'PROCESSOR_ARCHITECTURE')
    switch ($native.ToUpperInvariant()) {
        'AMD64' { return 'x64' }
        'ARM64' { return 'arm64' }
        'X86' { return 'x86' }
    }
    if (-not [Environment]::Is64BitOperatingSystem) {
        return 'x86'
    }
    return ''
}

# The Windows product (ApplicationID 55c92734-...) that has a key installed;
# LicenseStatus 1 is Licensed. Only LicenseStatus is selected, so the partial
# product key never leaves WMI. The query can take several seconds; any failure
# is "unknown".
function Get-ActivationCode {
    $query = "SELECT LicenseStatus FROM SoftwareLicensingProduct WHERE ApplicationID = '55c92734-d682-4d71-983e-d6ec3f16059f' AND PartialProductKey IS NOT NULL"
    try {
        $products = @(Get-CimInstance -Query $query)
    }
    catch {
        return 'unknown'
    }
    if ($products.Count -eq 0) {
        return 'unknown'
    }
    foreach ($p in $products) {
        if ((Get-Count $p.LicenseStatus) -eq 1) {
            return 'activated'
        }
    }
    return 'not-activated'
}

function Test-BasicDisplay {
    param($Controller, [string]$Name)
    if ($Name -match '(?i)\bBasic Display\b') {
        return $true
    }
    # "Microsoft ji ben xian shi shi pei qi" on Chinese systems, as \u escapes.
    if ($Name -match '\u57fa\u672c\u663e\u793a') {
        return $true
    }
    # The inbox basic display driver (section MSBDA of display.inf).
    return ((Get-CleanText $Controller.InfFilename) -eq 'display.inf')
}

# Dedicated video memory in bytes, 0 when it cannot be told reliably.
function Get-VideoMemory {
    param($Controller)
    $pnp = Get-CleanText $Controller.PNPDeviceID
    if ($pnp.Length -gt 0) {
        $driver = Get-CleanText (Get-RegistryValue -Path ('HKLM:\SYSTEM\CurrentControlSet\Enum\' + $pnp) -Name 'Driver')
        if ($driver -match ('^' + [regex]::Escape($displayClass) + '\\\d{4}$')) {
            $classKey = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\' + $driver
            $bytes = ConvertTo-Bytes (Get-RegistryValue -Path $classKey -Name 'HardwareInformation.qwMemorySize')
            if ($bytes -gt 0) {
                return $bytes
            }
            $bytes = ConvertTo-Bytes (Get-RegistryValue -Path $classKey -Name 'HardwareInformation.MemorySize')
            if (($bytes -gt 0) -and ($bytes -lt 1MB)) {
                return ($bytes * 1MB)
            }
            if (($bytes -gt 0) -and ($bytes -lt $saturated)) {
                return $bytes
            }
        }
    }
    $bytes = ConvertTo-Bytes $Controller.AdapterRAM
    if (($bytes -gt 0) -and ($bytes -lt $saturated)) {
        return $bytes
    }
    return [double]0
}

# Physical disks sort by their number.
function Get-DiskOrder {
    param($Disk)
    $n = 0
    if ([int]::TryParse([string]$Disk.DeviceId, [ref]$n)) {
        return $n
    }
    return [int]::MaxValue
}

# ---- computer
try {
    $rows = New-Object System.Collections.Generic.List[object]
    $cs = @(Get-CimInstance -ClassName Win32_ComputerSystem) | Select-Object -First 1
    $maker = Get-CleanText $cs.Manufacturer
    $model = Get-CleanText $cs.Model
    if ($maker -match '(?i)^lenovo') {
        $friendly = ''
        try {
            $product = @(Get-CimInstance -ClassName Win32_ComputerSystemProduct) | Select-Object -First 1
            $friendly = Get-CleanText $product.Version
        }
        catch {
            $friendly = ''
        }
        if ((Test-RealValue $friendly) -and ($friendly -notmatch '(?i)^lenovo( product)?$') -and ($friendly -ne $model)) {
            if (Test-RealValue $model) {
                $model = '{0} ({1})' -f $friendly, $model
            }
            else {
                $model = $friendly
            }
        }
    }
    if (Test-RealValue $maker) {
        $rows.Add((New-ValueRow -Id 'maker' -Value $maker))
    }
    if (Test-RealValue $model) {
        $rows.Add((New-ValueRow -Id 'model' -Value $model))
    }
    Add-Section -Id 'computer' -Name '' -Rows $rows
}
catch {
    Add-Failure 'computer'
}

# ---- os
$rows = New-Object System.Collections.Generic.List[object]
try {
    $os = @(Get-CimInstance -ClassName Win32_OperatingSystem) | Select-Object -First 1
    $caption = Get-CleanText $os.Caption
    if ($caption.Length -gt 0) {
        $rows.Add((New-ValueRow -Id 'name' -Value $caption))
    }
    $installed = Format-Date $os.InstallDate
}
catch {
    $installed = ''
    Add-Failure 'os'
}
try {
    $version = Get-WindowsVersion
    if ($version.Length -gt 0) {
        $rows.Add((New-ValueRow -Id 'version' -Value $version))
    }
    $arch = Get-ArchitectureCode
    if ($arch.Length -gt 0) {
        $rows.Add((New-CodeRow -Id 'arch' -Code $arch))
    }
}
catch {
    Add-Failure 'os'
}
if ($installed.Length -gt 0) {
    $rows.Add((New-ValueRow -Id 'install_date' -Value $installed))
}
$rows.Add((New-CodeRow -Id 'activation' -Code (Get-ActivationCode)))
Add-Section -Id 'os' -Name '' -Rows $rows

# ---- cpu
try {
    foreach ($cpu in @(Get-CimInstance -ClassName Win32_Processor)) {
        $rows = New-Object System.Collections.Generic.List[object]
        $cores = Get-Count $cpu.NumberOfCores
        if ($cores -gt 0) {
            $rows.Add((New-ValueRow -Id 'cores' -Value $cores))
        }
        $threads = Get-Count $cpu.NumberOfLogicalProcessors
        if ($threads -gt 0) {
            $rows.Add((New-ValueRow -Id 'threads' -Value $threads))
        }
        $clock = Get-Count $cpu.MaxClockSpeed
        if ($clock -gt 0) {
            $rows.Add((New-ValueRow -Id 'clock' -Value (Format-Clock $clock)))
        }
        Add-Section -Id 'cpu' -Name (Get-CleanText $cpu.Name) -Rows $rows
    }
}
catch {
    Add-Failure 'cpu'
}

# ---- board
$rows = New-Object System.Collections.Generic.List[object]
try {
    $board = @(Get-CimInstance -ClassName Win32_BaseBoard) | Select-Object -First 1
    $maker = Get-CleanText $board.Manufacturer
    if (Test-RealValue $maker) {
        $rows.Add((New-ValueRow -Id 'maker' -Value $maker))
    }
    $model = Get-CleanText $board.Product
    if (Test-RealValue $model) {
        $rows.Add((New-ValueRow -Id 'model' -Value $model))
    }
}
catch {
    Add-Failure 'board'
}
try {
    $bios = @(Get-CimInstance -ClassName Win32_BIOS) | Select-Object -First 1
    $biosVersion = Get-CleanText $bios.SMBIOSBIOSVersion
    if (Test-RealValue $biosVersion) {
        $rows.Add((New-ValueRow -Id 'bios_version' -Value $biosVersion))
    }
    $biosDate = Format-Date $bios.ReleaseDate
    if ($biosDate.Length -gt 0) {
        $rows.Add((New-ValueRow -Id 'bios_date' -Value $biosDate))
    }
}
catch {
    Add-Failure 'board'
}
Add-Section -Id 'board' -Name '' -Rows $rows

# ---- memory
$rows = New-Object System.Collections.Generic.List[object]
$modules = @()
try {
    $modules = @(Get-CimInstance -ClassName Win32_PhysicalMemory | Where-Object { (ConvertTo-Bytes $_.Capacity) -gt 0 })
}
catch {
    Add-Failure 'memory'
}
$total = [double]0
foreach ($m in $modules) {
    $total += ConvertTo-Bytes $m.Capacity
}
if ($total -le 0) {
    # Some virtual machines report no modules; this is the usable memory.
    try {
        $cs = @(Get-CimInstance -ClassName Win32_ComputerSystem) | Select-Object -First 1
        $total = ConvertTo-Bytes $cs.TotalPhysicalMemory
    }
    catch {
        Add-Failure 'memory'
    }
}
if ($total -gt 0) {
    $rows.Add((New-ValueRow -Id 'total' -Value (Format-MemorySize $total)))
}
if ($modules.Count -gt 0) {
    $slots = [long]0
    try {
        foreach ($array in @(Get-CimInstance -ClassName Win32_PhysicalMemoryArray)) {
            # Use 3 = system memory (others are video memory, flash, cache, ...).
            if (($null -ne $array.Use) -and ((Get-Count $array.Use) -ne 3)) {
                continue
            }
            $slots += Get-Count $array.MemoryDevices
        }
    }
    catch {
        $slots = [long]0
        Add-Failure 'memory'
    }
    if ($slots -gt 0) {
        # Firmware that reports fewer slots than modules is wrong.
        if ($slots -lt $modules.Count) {
            $slots = $modules.Count
        }
        $rows.Add((New-ValueRow -Id 'slots' -Value ('{0} / {1}' -f $modules.Count, $slots)))
    }
}
try {
    foreach ($m in $modules) {
        $parts = New-Object System.Collections.Generic.List[string]
        $parts.Add((Format-MemorySize (ConvertTo-Bytes $m.Capacity)))
        $type = Get-Count $m.SMBIOSMemoryType
        if ($memoryTypes.ContainsKey([int]$type)) {
            $parts.Add($memoryTypes[[int]$type])
        }
        $speed = Get-Count $m.Speed
        if ($speed -eq 0) {
            $speed = Get-Count $m.ConfiguredClockSpeed
        }
        if ($speed -gt 0) {
            $parts.Add(('{0} MHz' -f $speed))
        }
        $moduleMaker = Get-ReadableMaker (Get-CleanText $m.Manufacturer)
        if ($moduleMaker.Length -gt 0) {
            $parts.Add($moduleMaker)
        }
        $rows.Add((New-ValueRow -Id 'module' -Value ($parts -join ' ')))
    }
}
catch {
    Add-Failure 'memory'
}
Add-Section -Id 'memory' -Name '' -Rows $rows

# ---- gpu
try {
    foreach ($vc in @(Get-CimInstance -ClassName Win32_VideoController)) {
        $rows = New-Object System.Collections.Generic.List[object]
        $name = Get-CleanText $vc.Name
        if (Test-BasicDisplay -Controller $vc -Name $name) {
            $rows.Add((New-CodeRow -Id 'driver_status' -Code 'basic-driver'))
        }
        else {
            $vram = Get-VideoMemory $vc
            if ($vram -gt 0) {
                $rows.Add((New-ValueRow -Id 'vram' -Value (Format-MemorySize $vram)))
            }
        }
        $driverVersion = Get-CleanText $vc.DriverVersion
        if ($driverVersion.Length -gt 0) {
            $rows.Add((New-ValueRow -Id 'driver_version' -Value $driverVersion))
        }
        $driverDate = Format-Date $vc.DriverDate
        if ($driverDate.Length -gt 0) {
            $rows.Add((New-ValueRow -Id 'driver_date' -Value $driverDate))
        }
        $width = Get-Count $vc.CurrentHorizontalResolution
        $height = Get-Count $vc.CurrentVerticalResolution
        if (($width -gt 0) -and ($height -gt 0)) {
            # U+00D7 is the multiplication sign.
            $resolution = '{0} {1} {2}' -f $width, [char]0x00D7, $height
            $hz = Get-Count $vc.CurrentRefreshRate
            if ($hz -gt 1) {
                $resolution += (', {0} Hz' -f $hz)
            }
            $rows.Add((New-ValueRow -Id 'resolution' -Value $resolution))
        }
        Add-Section -Id 'gpu' -Name $name -Rows $rows
    }
}
catch {
    Add-Failure 'gpu'
}

# ---- disk
try {
    foreach ($disk in @(Get-PhysicalDisk | Sort-Object -Property @{ Expression = { Get-DiskOrder $_ } })) {
        $rows = New-Object System.Collections.Generic.List[object]
        $size = ConvertTo-Bytes $disk.Size
        if ($size -gt 0) {
            $rows.Add((New-ValueRow -Id 'size' -Value (Format-DiskSize $size)))
        }
        $media = ConvertTo-Name -Value $disk.MediaType -Names $mediaNames
        $type = 'unknown'
        if (($media -eq 'SSD') -or ($media -eq 'SCM')) {
            $type = 'ssd'
        }
        elseif ($media -eq 'HDD') {
            $type = 'hdd'
        }
        $rows.Add((New-CodeRow -Id 'type' -Code $type))
        $bus = ConvertTo-Name -Value $disk.BusType -Names $busNames
        if (($bus.Length -gt 0) -and ($bus -ne 'Unknown')) {
            $rows.Add((New-ValueRow -Id 'bus' -Value $bus))
        }
        Add-Section -Id 'disk' -Name (Get-CleanText $disk.FriendlyName) -Rows $rows
    }
}
catch {
    Add-Failure 'disk'
}

# ---- network
try {
    foreach ($adapter in @(Get-NetAdapter -Physical | Sort-Object -Property InterfaceDescription)) {
        $rows = New-Object System.Collections.Generic.List[object]
        # NdisPhysicalMedium: 1 wireless LAN, 9 native 802.11, 14 802.3.
        # InterfaceType (IANA ifType): 6 ethernetCsmacd, 71 ieee80211.
        $medium = Get-Count $adapter.NdisPhysicalMedium
        $ifType = Get-Count $adapter.InterfaceType
        $kind = 'other'
        if (($medium -eq 9) -or ($medium -eq 1) -or ($ifType -eq 71)) {
            $kind = 'wifi'
        }
        elseif (($medium -eq 14) -or (($medium -eq 0) -and ($ifType -eq 6))) {
            $kind = 'ethernet'
        }
        $rows.Add((New-CodeRow -Id 'kind' -Code $kind))

        # State 3 = disabled; InterfaceOperationalStatus 1 = up;
        # MediaConnectState 1 = connected. The Status text Get-NetAdapter shows
        # ("Up", "Disconnected", "Disabled") is the fallback.
        $status = 'down'
        $statusText = Get-CleanText $adapter.Status
        if (((Get-Count $adapter.State) -eq 3) -or ($statusText -eq 'Disabled')) {
            $status = 'disabled'
        }
        elseif (((Get-Count $adapter.InterfaceOperationalStatus) -eq 1) -or ((Get-Count $adapter.MediaConnectState) -eq 1) -or ($statusText -eq 'Up')) {
            $status = 'up'
        }
        $rows.Add((New-CodeRow -Id 'status' -Code $status))
        Add-Section -Id 'network' -Name (Get-CleanText $adapter.InterfaceDescription) -Rows $rows
    }
}
catch {
    Add-Failure 'network'
}

# The os section always has the activation row; without anything else there is
# nothing worth showing.
$useful = @($sections | Where-Object { ($_['id'] -ne 'os') -or (@($_['rows']).Count -gt 1) })
if ($useful.Count -eq 0) {
    throw ('Could not read any hardware information (failed: {0})' -f ($failed -join ', '))
}

$result = 'ok'
if ($failed.Count -gt 0) {
    $result = 'partial'
}

[pscustomobject]@{
    result   = $result
    facts    = [ordered]@{
        section_count = $sections.Count
        failed_count  = $failed.Count
    }
    sections = $sections.ToArray()
}
