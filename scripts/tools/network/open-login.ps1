[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
# Windows uses this HTTP endpoint to let captive portals redirect the browser
# to their sign-in page. Opening it does not change any network setting.
$result = 'opened'
try {
    Start-Process -FilePath 'http://www.msftconnecttest.com/redirect'
}
catch {
    $result = 'no-browser'
}

[pscustomobject]@{
    result = $result
    facts  = @{}
}
