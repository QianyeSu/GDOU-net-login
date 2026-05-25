$ErrorActionPreference = "Stop"

$configPath = Join-Path $env:APPDATA "gdou\gdou-net-login\config\config.json"
if (-not (Test-Path $configPath)) {
  throw "Config not found: $configPath"
}

$config = Get-Content $configPath -Encoding UTF8 | ConvertFrom-Json
$portal = [string]$config.portal_url
if ([string]::IsNullOrWhiteSpace($portal)) {
  throw "portal_url is empty in config"
}

$portal = $portal.TrimEnd("/")
$timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$callback = "jQuery1124_$timestamp"

function Invoke-ProbeUrl {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$Url
  )

  $watch = [Diagnostics.Stopwatch]::StartNew()
  try {
    $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 5
    $watch.Stop()
    $body = [string]$response.Content
    if ($body.Length -gt 800) {
      $body = $body.Substring(0, 800)
    }
    [pscustomobject]@{
      Name = $Name
      Ms = [int]$watch.ElapsedMilliseconds
      Status = $response.StatusCode
      Body = $body
    }
  } catch {
    $watch.Stop()
    [pscustomobject]@{
      Name = $Name
      Ms = [int]$watch.ElapsedMilliseconds
      Status = "ERR"
      Body = $_.Exception.Message
    }
  }
}

Write-Host "GDOU quick probe (read-only)"
Write-Host "Config: portal=$($config.portal_url) ac_id=$($config.ac_id) ip=$($config.user_ip) user=$($config.username)"
Write-Host ""

$stateUrl = "$portal/cgi-bin/rad_user_info?callback=$callback&_=$timestamp"
$challengeUrl = "$portal/cgi-bin/get_challenge?callback=$callback&username=$([uri]::EscapeDataString([string]$config.username))&ip=$($config.user_ip)&ac_id=$($config.ac_id)&_=$timestamp"

Invoke-ProbeUrl "rad_user_info" $stateUrl | Format-List
Invoke-ProbeUrl "get_challenge" $challengeUrl | Format-List
