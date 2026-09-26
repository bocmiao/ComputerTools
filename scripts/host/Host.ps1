#Requires -Version 5.1
<#
.SYNOPSIS
    medkit script host.

.DESCRIPTION
    Reads one JSON request per line from stdin and writes one JSON response per
    line to stdout (see docs/architecture.md, section 5.3):

        -> {"id":1,"script":"checks/disk/system-free-space.ps1","args":{}}
        <- {"id":1,"ok":true,"data":{...},"ms":84}
        <- {"id":2,"ok":false,"error":"...","ms":12}

    On start-up it writes {"id":0,"ready":true,"ps":"5.1..."} before reading
    any request.

    Only the success stream of a script is collected. Warning, verbose, debug and
    information streams are discarded so that stray output cannot break the
    protocol. The engine verifies the SHA-256 of every script before sending a
    request; this host additionally refuses paths outside its root.

    This file must stay ASCII-only and compatible with Windows PowerShell 5.1.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$utf8 = New-Object System.Text.UTF8Encoding $false
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$rootPrefix = $root.TrimEnd([char[]]@('\', '/')) + [System.IO.Path]::DirectorySeparatorChar

function Write-Response {
    param([System.Collections.IDictionary]$Response)
    $json = ConvertTo-Json -InputObject $Response -Depth 10 -Compress
    [Console]::Out.WriteLine($json)
    [Console]::Out.Flush()
}

function Resolve-ScriptPath {
    param([string]$Relative)
    if ([string]::IsNullOrWhiteSpace($Relative)) { throw 'empty script path' }
    if ($Relative -notmatch '^[A-Za-z0-9_\-]+(/[A-Za-z0-9_\-\.]+)*\.ps1$' -or $Relative -match '\.\.') {
        throw "invalid script path: $Relative"
    }
    $full = [System.IO.Path]::GetFullPath((Join-Path $root $Relative))
    if (-not $full.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "script outside root: $Relative"
    }
    if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "script not found: $Relative" }
    return $full
}

# Tell the engine we are ready, so that PowerShell start-up time (which can be
# several seconds on an old machine) is not counted against the first script.
Write-Response -Response ([ordered]@{ id = 0; ready = $true; ps = $PSVersionTable.PSVersion.ToString() })

while ($true) {
    $line = [Console]::In.ReadLine()
    if ($null -eq $line) { break }
    if ($line.Trim().Length -eq 0) { continue }

    $id = $null
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $request = ConvertFrom-Json -InputObject $line
        $id = $request.id
        $path = Resolve-ScriptPath -Relative ([string]$request.script)

        $params = @{}
        if ($null -ne $request.args) {
            foreach ($p in $request.args.PSObject.Properties) { $params[$p.Name] = $p.Value }
        }

        $output = @(& $path @params 3>$null 4>$null 5>$null 6>$null)

        $response = [ordered]@{ id = $id; ok = $true; data = $null; ms = $watch.ElapsedMilliseconds }
        if ($output.Count -ge 1) { $response.data = $output[$output.Count - 1] }
        if ($output.Count -gt 1) { $response.extra_output = $output.Count - 1 }
        Write-Response -Response $response
    }
    catch {
        $message = $_.Exception.Message
        if ([string]::IsNullOrWhiteSpace($message)) { $message = [string]$_ }
        Write-Response -Response ([ordered]@{ id = $id; ok = $false; error = $message; ms = $watch.ElapsedMilliseconds })
    }
}
