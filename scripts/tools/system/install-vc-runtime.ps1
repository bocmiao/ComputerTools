# Install the Microsoft Visual C++ Redistributable (v14): download the latest
# one from Microsoft's permanent links (https://aka.ms/vc14/vc_redist.<arch>.exe;
# Microsoft, "Latest supported Visual C++ Redistributable downloads") and run
# it quietly: /install /quiet /norestart ("Redistribute Visual C++ files").
# This is the one kind of download scripts may do (docs/architecture.md 5.1):
# - only from Microsoft: the links above, and every redirect must end on an
#   https address under microsoft.com;
# - into a new folder under %SystemRoot%\Temp that only SYSTEM and
#   Administrators can open, so a program running without administrator
#   rights cannot swap the file after its signature was checked;
# - a file is run only when its Authenticode signature is valid and it is
#   signed by Microsoft Corporation. The folder is deleted afterwards.
# Both installers are downloaded first (at most 5 minutes; nothing is changed
# when that fails), then run. One whose version is already installed with all
# its files is not run; one whose version is installed but whose files are
# gone is run with /repair.
# Installer exit codes: 0 done, 3010 done but Windows must restart, 1638 this
# or a newer version is already installed, 1618 another installation is
# running.
# Result codes: installed / restart / already / busy / download-failed /
# not-signed / failed / slow (an installer was still running after 9 minutes:
# it is left to finish). Facts: done (the architectures installed or repaired,
# comma separated), arch (the one that failed), code (its exit code).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# ---- shared block vc-runtime: identical in checks/system/vc-runtime.ps1 and tools/system/install-vc-runtime.ps1 (medkit-data check compares them) ----
# The Microsoft Visual C++ Redistributable v14 (Visual Studio 2015 to 2026).
# 32-bit programs need the x86 one, 64-bit programs the x64 one: 64-bit
# Windows needs both (on ARM64 Windows the x64 package also brings the ARM64
# files), 32-bit Windows only x86.
function Get-VcArchitecture {
    if ([Environment]::Is64BitOperatingSystem) {
        return @('x64', 'x86')
    }
    return @('x86')
}

# One architecture: the installed version ('' when it is not installed) from
# HKLM\SOFTWARE[\WOW6432Node]\Microsoft\VisualStudio\14.0\VC\Runtimes\<arch>
# (Installed = 1, Version "v14.44.35211.00"; Microsoft, "Redistribute Visual
# C++ files"), and whether the files programs load are there (vcruntime140.dll
# and msvcp140.dll, in SysWOW64 for x86 on 64-bit Windows).
function Get-VcRuntime {
    param([string]$Arch)
    $version = ''
    foreach ($root in @('HKLM:\SOFTWARE\Microsoft', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft')) {
        try {
            $key = Get-ItemProperty -LiteralPath ($root + '\VisualStudio\14.0\VC\Runtimes\' + $Arch) -ErrorAction Stop
        }
        catch {
            continue
        }
        if ($key.Installed -ne 1) {
            continue
        }
        $text = ''
        if ($key.Version -is [string]) {
            $text = $key.Version.Trim().TrimStart('v', 'V')
        }
        elseif ($null -ne $key.Major) {
            $text = '{0}.{1}.{2}' -f $key.Major, $key.Minor, $key.Bld
        }
        if ($text.Length -eq 0) {
            $text = '14.0'
        }
        $version = $text
        break
    }
    $windows = ([string]$env:SystemRoot).TrimEnd('\')
    $folder = $windows + '\System32'
    if (($Arch -eq 'x86') -and [Environment]::Is64BitOperatingSystem) {
        $folder = $windows + '\SysWOW64'
    }
    $files = $true
    foreach ($name in @('vcruntime140.dll', 'msvcp140.dll')) {
        if (-not (Test-Path -LiteralPath ($folder + '\' + $name) -PathType Leaf)) {
            $files = $false
        }
    }
    return [pscustomobject]@{ arch = $Arch; version = $version; files = $files }
}

# Older than 14.40 (Visual Studio 2022 17.10): programs built with newer tools can crash with it.
function Test-VcOld {
    param([string]$Version)
    try {
        return ([version]$Version) -lt ([version]'14.40')
    }
    catch {
        return $false
    }
}
# ---- end of shared block vc-runtime ----

$started = [DateTime]::UtcNow
$downloadBy = $started.AddMinutes(5)
$installBy = $started.AddMinutes(9)

# Microsoft's download servers want TLS 1.2
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}
catch {
    Write-Verbose ('TLS 1.2 could not be turned on: {0}' -f $_.Exception.Message)
}

# A new folder only SYSTEM and Administrators can open (owner Administrators,
# not inheriting from %SystemRoot%\Temp, where every user can create files)
function New-WorkFolder {
    $path = ([string]$env:SystemRoot).TrimEnd('\') + '\Temp\medkit-vc-' + [guid]::NewGuid().ToString('N')
    $security = New-Object System.Security.AccessControl.DirectorySecurity
    $security.SetSecurityDescriptorSddlForm('O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)')
    $null = [IO.Directory]::CreateDirectory($path, $security)
    return $path
}

# Download $Uri to $Path; give up at $Deadline, or when a redirect leaves
# https://*.microsoft.com, or after 200 MB (the installers are 7 and 19 MB).
function Save-Installer {
    param([string]$Uri, [string]$Path, [DateTime]$Deadline)
    $request = [Net.WebRequest]::Create($Uri)
    $request.Timeout = 30000
    $request.ReadWriteTimeout = 30000
    $response = $request.GetResponse()
    try {
        $final = $response.ResponseUri
        $hostName = ([string]$final.Host).ToLowerInvariant()
        if (($final.Scheme -ne 'https') -or -not (($hostName -eq 'microsoft.com') -or $hostName.EndsWith('.microsoft.com'))) {
            throw ('Redirected away from Microsoft: {0}' -f $hostName)
        }
        $stream = $response.GetResponseStream()
        $out = [IO.File]::Create($Path)
        try {
            $buffer = New-Object byte[] 65536
            $total = 0
            while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                $out.Write($buffer, 0, $read)
                $total += $read
                if ($total -gt 200MB) {
                    throw 'The download is too large'
                }
                if ([DateTime]::UtcNow -gt $Deadline) {
                    throw 'The download is too slow'
                }
            }
        }
        finally {
            $out.Dispose()
            $stream.Dispose()
        }
    }
    finally {
        $response.Close()
    }
}

function Test-MicrosoftSigned {
    param([string]$Path)
    try {
        $signature = Get-AuthenticodeSignature -FilePath $Path
    }
    catch {
        return $false
    }
    if ([string]$signature.Status -ne 'Valid') {
        return $false
    }
    return ([string]$signature.SignerCertificate.Subject) -match '(^|,\s*)O=Microsoft Corporation(,|$)'
}

# The version an installer brings (its file version: 14.51.36247.0), $null when unknown
function Get-InstallerVersion {
    param([string]$Path)
    $info = (Get-Item -LiteralPath $Path).VersionInfo
    if ($info.FileMajorPart -ne 14) {
        return $null
    }
    return New-Object Version($info.FileMajorPart, $info.FileMinorPart, $info.FileBuildPart, $info.FilePrivatePart)
}

# Run an installer quietly; its exit code, or $null when it is still running at $Deadline.
# Started through the shell, so it does not inherit this host's pipes to the app.
function Invoke-Installer {
    param([string]$Path, [string]$Mode, [DateTime]$Deadline)
    $start = New-Object Diagnostics.ProcessStartInfo($Path, ($Mode + ' /quiet /norestart'))
    $start.UseShellExecute = $true
    $start.WorkingDirectory = Split-Path -Parent $Path
    $process = [Diagnostics.Process]::Start($start)
    $wait = [int][Math]::Max(1000, ($Deadline - [DateTime]::UtcNow).TotalMilliseconds)
    if (-not $process.WaitForExit($wait)) {
        return $null
    }
    return [int]$process.ExitCode
}

$result = 'already'
$facts = [ordered]@{ done = ''; arch = ''; code = '' }
$done = New-Object System.Collections.Generic.List[string]
$restart = $false
$work = New-WorkFolder
try {
    $files = [ordered]@{}
    foreach ($arch in @(Get-VcArchitecture)) {
        $file = $work + '\vc_redist.' + $arch + '.exe'
        try {
            Save-Installer -Uri ('https://aka.ms/vc14/vc_redist.' + $arch + '.exe') -Path $file -Deadline $downloadBy
        }
        catch {
            Write-Verbose ('Download failed: {0}' -f $_.Exception.Message)
            $result = 'download-failed'
            $facts.arch = $arch
            break
        }
        if (-not (Test-MicrosoftSigned $file)) {
            $result = 'not-signed'
            $facts.arch = $arch
            break
        }
        $files[$arch] = $file
    }

    if ($result -eq 'already') {
        foreach ($arch in @($files.Keys)) {
            $file = $files[$arch]
            $state = Get-VcRuntime $arch
            $offered = Get-InstallerVersion $file
            $mode = '/install'
            if (($null -ne $offered) -and ($state.version.Length -gt 0)) {
                $installed = $null
                try {
                    $installed = [version]$state.version
                }
                catch {
                    Write-Verbose ('Unreadable installed version: {0}' -f $state.version)
                }
                if (($null -ne $installed) -and ($installed -ge $offered)) {
                    if ($state.files) {
                        continue
                    }
                    $mode = '/repair'
                }
            }
            $code = Invoke-Installer $file $mode $installBy
            if (($code -eq 1638) -and -not (Get-VcRuntime $arch).files) {
                $code = Invoke-Installer $file '/repair' $installBy
            }
            if ($null -eq $code) {
                $result = 'slow'
                $facts.arch = $arch
                break
            }
            if (($code -eq 0) -or ($code -eq 3010)) {
                $done.Add($arch)
                if ($code -eq 3010) {
                    $restart = $true
                }
                continue
            }
            if ($code -eq 1638) {
                continue
            }
            $facts.arch = $arch
            $facts.code = [string]$code
            if ($code -eq 1618) {
                $result = 'busy'
            }
            else {
                $result = 'failed'
            }
            break
        }
    }
}
finally {
    # An installer that is still running keeps its file: that one stays until Windows cleans up Temp
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

$facts.done = $done.ToArray() -join ', '
if (($result -eq 'already') -and ($done.Count -gt 0)) {
    $result = 'installed'
    if ($restart) {
        $result = 'restart'
    }
}

[pscustomobject]@{
    result = $result
    facts  = $facts
}
