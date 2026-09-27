param(
    [Parameter(Mandatory=$true)]
    [string]$FilePath,
    [string]$CertificateThumbprint = $env:WINDOWS_CERTIFICATE_THUMBPRINT
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
# Tauri invokes Windows PowerShell with -NoProfile. Explicitly load the
# certificate provider so the same script works there and in pwsh smoke tests.
Import-Module Microsoft.PowerShell.Security -ErrorAction Stop

if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
    throw "File to sign not found: $FilePath"
}
$FilePath = (Get-Item -LiteralPath $FilePath).FullName

Write-Host "==> Signing Windows binary: $FilePath"

# Pin the certificate selected by CI; never silently pick an unrelated identity.
if ([string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
    throw "Set WINDOWS_CERTIFICATE_THUMBPRINT or pass -CertificateThumbprint before signing."
}
$cert = Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
if (-not $cert.HasPrivateKey) {
    throw "The selected code signing certificate has no private key."
}

Write-Host "Using certificate: $($cert.Subject) [$($cert.Thumbprint)]"

# Set-AuthenticodeSignature uses Authenticode timestamping, not RFC 3161.
$timestampServers = @(
    "http://timestamp.digicert.com",
    "http://timestamp.sectigo.com",
    "http://tsa.starfieldtech.com"
)

$signed = $false
foreach ($ts in $timestampServers) {
    try {
        $result = Set-AuthenticodeSignature -FilePath $FilePath -Certificate $cert -TimestampServer $ts -HashAlgorithm SHA256 -ErrorAction Stop
        if ($result.Status -eq "Valid" -and $null -ne $result.TimeStamperCertificate) {
            Write-Host "Successfully signed with timestamp from: $ts"
            $signed = $true
            break
        }
        Write-Warning "Timestamp attempt at ${ts}: $($result.Status) - $($result.StatusMessage)"
    } catch {
        Write-Warning "Timestamp server $ts failed or timed out: $_"
    }
}

if (-not $signed) {
    Write-Warning "No verified timestamp was obtained; signing without a timestamp. Do not promise validity beyond certificate expiry."
    $result = Set-AuthenticodeSignature -FilePath $FilePath -Certificate $cert -HashAlgorithm SHA256
}

# Re-read the actual file. CI trusts a self-signed public certificate only inside
# its disposable runner; Valid here does NOT imply public Windows/SmartScreen trust.
$signature = Get-AuthenticodeSignature -LiteralPath $FilePath
if ($signature.Status -ne "Valid" -or $null -eq $signature.SignerCertificate -or
    $signature.SignerCertificate.Thumbprint -ne $cert.Thumbprint) {
    throw "Signature verification failed: $($signature.Status) - $($signature.StatusMessage)"
}
Write-Host "Verified signature against selected certificate: $($cert.Thumbprint); timestamp present: $($null -ne $signature.TimeStamperCertificate)"
