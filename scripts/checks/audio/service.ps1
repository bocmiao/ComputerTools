# Check: audio.service
# Windows Audio (Audiosrv) and Windows Audio Endpoint Builder
# (AudioEndpointBuilder). Both start automatically on Windows; without them
# there is no sound at all.
# Read-only. Outputs one object: { result, facts }.
# Result codes (in this order):
#   missing   one of the services does not exist
#   disabled  one of them is set to Disabled (fix: audio.enable-services)
#   stopped   one of them is not running (tool: audio.restart-service)
#   ok
# Facts: audiosrv, builder (Running, Stopped, ...), audiosrv_start,
# builder_start (Automatic, Manual, Disabled).

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$audio = Get-Service -Name 'Audiosrv' -ErrorAction SilentlyContinue
$builder = Get-Service -Name 'AudioEndpointBuilder' -ErrorAction SilentlyContinue
if (($null -eq $audio) -or ($null -eq $builder)) {
    [pscustomobject]@{ result = 'missing'; facts = [ordered]@{} }
    return
}

$result = 'ok'
if (([string]$audio.StartType -eq 'Disabled') -or ([string]$builder.StartType -eq 'Disabled')) {
    $result = 'disabled'
}
elseif (([string]$audio.Status -ne 'Running') -or ([string]$builder.Status -ne 'Running')) {
    $result = 'stopped'
}

[pscustomobject]@{
    result = $result
    facts  = [ordered]@{
        audiosrv       = [string]$audio.Status
        audiosrv_start = [string]$audio.StartType
        builder        = [string]$builder.Status
        builder_start  = [string]$builder.StartType
    }
}
