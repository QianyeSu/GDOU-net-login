param(
    [Parameter(Mandatory=$true)]
    [string]$FilePath
)

if (-not (Test-Path $FilePath)) {
    Write-Warning "File to sign not found: $FilePath"
    exit 0
}

Write-Host "==> Signing Windows binary: $FilePath"

# Locate code signing certificate in CurrentUser\My
$cert = Get-ChildItem -Path Cert:\CurrentUser\My -CodeSigningCert | Select-Object -First 1
if (-not $cert) {
    Write-Warning "No code signing certificate found in Cert:\CurrentUser\My. Skipping signing."
    exit 0
}

Write-Host "Using certificate: $($cert.Subject) [$($cert.Thumbprint)]"

# Standard RFC 3161 timestamp authorities with fallbacks
$timestampServers = @(
    "http://timestamp.digicert.com",
    "http://timestamp.sectigo.com",
    "http://tsa.starfieldtech.com"
)

$signed = $false
foreach ($ts in $timestampServers) {
    try {
        $result = Set-AuthenticodeSignature -FilePath $FilePath -Certificate $cert -TimestampServer $ts -HashAlgorithm SHA256 -ErrorAction Stop
        if ($result.Status -eq "Valid") {
            Write-Host "Successfully signed with timestamp from: $ts"
            $signed = $true
            break
        }
    } catch {
        Write-Warning "Timestamp server $ts failed or timed out: $_"
    }
}

if (-not $signed) {
    Write-Host "Signing without timestamp as fallback..."
    $result = Set-AuthenticodeSignature -FilePath $FilePath -Certificate $cert -HashAlgorithm SHA256
    Write-Host "Signature result: $($result.Status)"
}
