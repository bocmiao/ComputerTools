# Tool: system.edge-repair (action)
# Opens Microsoft Edge's own repair, the same as Settings > Apps > Installed
# apps > Microsoft Edge > Modify (Microsoft Support, "What to do if Microsoft
# Edge isn't working": close Edge for all users, Modify, Repair; browser data
# and settings should not be affected; when Modify is not there, an
# organization installed Edge and manages it).
# Modify runs the ModifyPath Edge registered under
# SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Microsoft Edge (32-bit
# registry view). Read on the CI runner, it is Microsoft Edge Update doing an
# online repair:
#   "C:\Program Files (x86)\Microsoft\EdgeUpdate\MicrosoftEdgeUpdate.exe"
#   /install appguid={56EB18F8-B008-4CBD-B6D2-8C97FE7E9062}&appname=Microsoft%20Edge
#   &needsadmin=true&repairtype=windowsonlinerepair /installsource windows
# The uninstall command is another program (setup.exe --uninstall) and is
# never run. Before starting, the command is checked: MicrosoftEdgeUpdate.exe
# under Program Files (x86) or Program Files \Microsoft\EdgeUpdate, the file
# exists and carries a valid signature by Microsoft Corporation (written like
# install-vc-runtime.ps1); the arguments start with /install, carry Edge
# Stable's app id and repairtype=windowsonlinerepair, never "uninstall", and
# only hold letters, digits, spaces and = . _ - / { } & %.
# The repair downloads Edge again (needs the internet) and needs administrator
# rights (medkit runs elevated). It is not waited for.
# Results: started / no-edge / no-repair (NoModify, or no Modify command: an
# organization manages Edge) / missing (Edge Update is gone) / untrusted.
# Facts: name (the registered display name), version.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

# The "Microsoft Edge" entry of Apps (32-bit view first: Edge is a 32-bit
# install on 64-bit Windows), or $null
function Get-EdgeEntry {
    foreach ($view in @([Microsoft.Win32.RegistryView]::Registry32, [Microsoft.Win32.RegistryView]::Registry64)) {
        $base = $null
        $key = $null
        try {
            $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $view)
            $key = $base.OpenSubKey('SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Microsoft Edge')
            if ($null -ne $key) {
                return [pscustomobject]@{
                    Name      = [string]$key.GetValue('DisplayName')
                    Version   = [string]$key.GetValue('DisplayVersion')
                    Publisher = [string]$key.GetValue('Publisher')
                    Modify    = [string]$key.GetValue('ModifyPath')
                    NoModify  = ($key.GetValue('NoModify') -eq 1)
                }
            }
        }
        catch {
            Write-Verbose ('Cannot read the Apps entry: {0}' -f $_.Exception.Message)
        }
        finally {
            if ($null -ne $key) {
                $key.Close()
            }
            if ($null -ne $base) {
                $base.Close()
            }
        }
    }
    return $null
}

# The program and the arguments of a registered command line: a quoted program
# first, else everything up to the first ".exe". $null when there is neither.
function Split-CommandLine {
    param([string]$Command)
    $text = $Command.Trim()
    if ($text.StartsWith('"')) {
        $close = $text.IndexOf('"', 1)
        if ($close -lt 2) {
            return $null
        }
        return [pscustomobject]@{ File = $text.Substring(1, $close - 1); Arguments = $text.Substring($close + 1).Trim() }
    }
    $exe = $text.IndexOf('.exe', [System.StringComparison]::OrdinalIgnoreCase)
    if ($exe -lt 1) {
        return $null
    }
    return [pscustomobject]@{ File = $text.Substring(0, $exe + 4); Arguments = $text.Substring($exe + 4).Trim() }
}

# '' when $Path carries a valid Authenticode signature by Microsoft Corporation;
# else why not: the signature status (NotSigned, HashMismatch, UnknownError...),
# NotMicrosoft, or the error
function Get-SignatureProblem {
    param([string]$Path)
    try {
        $signature = Get-AuthenticodeSignature -FilePath $Path
    }
    catch {
        return ('Error ' + $_.Exception.GetType().Name)
    }
    if ([string]$signature.Status -ne 'Valid') {
        return [string]$signature.Status
    }
    if (([string]$signature.SignerCertificate.Subject) -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
        return 'NotMicrosoft'
    }
    return ''
}

# Where Microsoft Edge Update is installed (64- or 32-bit Program Files).
function Get-EdgeUpdateFolder {
    $folders = New-Object System.Collections.Generic.List[string]
    foreach ($name in @('ProgramFilesX86', 'ProgramFiles')) {
        $root = [Environment]::GetFolderPath($name)
        if (-not [string]::IsNullOrWhiteSpace($root)) {
            $folders.Add(([System.IO.Path]::GetFullPath((Join-Path $root 'Microsoft\EdgeUpdate'))).TrimEnd('\') + '\')
        }
    }
    return $folders.ToArray()
}

# True when $Arguments is Edge Update's repair of Edge Stable and nothing else.
function Test-RepairArgument {
    param([string]$Arguments)
    if ($Arguments -notmatch '^[A-Za-z0-9 =._/{}&%\-]*$') {
        return $false
    }
    if ($Arguments -match 'uninstall') {
        return $false
    }
    # Edge Stable's app id in Microsoft Edge Update
    $appId = [regex]::Escape('{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062}')
    return (($Arguments -match '^/install\s') -and
        ($Arguments -match ('(^|[\s&])appguid=' + $appId + '(&|\s|$)')) -and
        ($Arguments -match '(^|[\s&])repairtype=windowsonlinerepair(&|\s|$)'))
}

# What to do: Code is start (File and Arguments are the checked repair
# command) or one of the result codes. Nothing is started here, so the Windows
# tests can check a real machine without repairing its Edge.
function Get-RepairPlan {
    $plan = [pscustomobject]@{ Code = 'no-edge'; Name = ''; Version = ''; File = ''; Arguments = '' }
    $edge = Get-EdgeEntry
    if (($null -eq $edge) -or ($edge.Publisher -ne 'Microsoft Corporation')) {
        return $plan
    }
    $plan.Name = $edge.Name
    if ([string]::IsNullOrWhiteSpace($plan.Name)) {
        $plan.Name = 'Microsoft Edge'
    }
    $plan.Version = $edge.Version
    $plan.Code = 'untrusted'
    if ($edge.NoModify -or [string]::IsNullOrWhiteSpace($edge.Modify)) {
        $plan.Code = 'no-repair'
        return $plan
    }
    $parts = Split-CommandLine -Command $edge.Modify
    if ($null -eq $parts) {
        return $plan
    }
    $file = [Environment]::ExpandEnvironmentVariables($parts.File)
    if (-not [System.IO.Path]::IsPathRooted($file)) {
        return $plan
    }
    $file = [System.IO.Path]::GetFullPath($file)
    $allowed = $false
    foreach ($folder in @(Get-EdgeUpdateFolder)) {
        if ($file.StartsWith($folder, [System.StringComparison]::OrdinalIgnoreCase)) {
            $allowed = $true
        }
    }
    $leaf = [System.IO.Path]::GetFileName($file)
    if ((-not $allowed) -or ($leaf -ne 'MicrosoftEdgeUpdate.exe') -or (-not (Test-RepairArgument -Arguments $parts.Arguments))) {
        return $plan
    }
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        $plan.Code = 'missing'
        return $plan
    }
    $problem = Get-SignatureProblem -Path $file
    if ($problem.Length -gt 0) {
        Write-Verbose ('Not started, signature: {0}' -f $problem)
        return $plan
    }
    $plan.Code = 'start'
    $plan.File = $file
    $plan.Arguments = $parts.Arguments
    return $plan
}

function New-Result {
    param([string]$Code, [string]$Name, [string]$Version)
    return [pscustomobject]@{
        result = $Code
        facts  = [ordered]@{
            name    = $Name
            version = $Version
        }
    }
}

$plan = Get-RepairPlan
if ($plan.Code -ne 'start') {
    New-Result -Code $plan.Code -Name $plan.Name -Version $plan.Version
    return
}
Start-Process -FilePath $plan.File -ArgumentList $plan.Arguments
New-Result -Code 'started' -Name $plan.Name -Version $plan.Version
