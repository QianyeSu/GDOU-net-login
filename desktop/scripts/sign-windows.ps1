param(
    [Parameter(Mandatory=$true)]
    [string]$FilePath,
    [string]$CertificateThumbprint = $env:WINDOWS_CERTIFICATE_THUMBPRINT
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
    throw "File to sign not found: $FilePath"
}
$FilePath = (Get-Item -LiteralPath $FilePath).FullName
if ([string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
    throw "Set WINDOWS_CERTIFICATE_THUMBPRINT before signing."
}
$CertificateThumbprint = $CertificateThumbprint.Replace(' ', '').ToUpperInvariant()

# Use the Windows SDK signer instead of Set-AuthenticodeSignature. Tauri calls
# this script with Windows PowerShell -NoProfile, where the PowerShell Security
# module may fail to load on hosted runners even though signtool is available.
$signtool = Get-ChildItem -LiteralPath "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
if ($null -eq $signtool) { throw "Windows SDK signtool.exe was not found." }

$store = [System.Security.Cryptography.X509Certificates.X509Store]::new(
    [System.Security.Cryptography.X509Certificates.StoreName]::My,
    [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser)
$store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly)
try {
    $cert = $store.Certificates | Where-Object { $_.Thumbprint -eq $CertificateThumbprint } | Select-Object -First 1
} finally {
    $store.Close()
}
if ($null -eq $cert) { throw "Signing certificate not found in CurrentUser\\My: $CertificateThumbprint" }
if (-not $cert.HasPrivateKey) { throw "The selected code signing certificate has no private key." }

Write-Host "==> Signing Windows binary: $FilePath"
Write-Host "Using certificate: $($cert.Subject) [$CertificateThumbprint]"
Write-Host "Using signtool: $($signtool.FullName)"

$timestampServers = @(
    "http://timestamp.digicert.com",
    "http://timestamp.sectigo.com",
    "http://tsa.starfieldtech.com"
)
$signed = $false
foreach ($ts in $timestampServers) {
    & $signtool.FullName sign /fd SHA256 /sha1 $CertificateThumbprint /tr $ts /td SHA256 /d "GDOU Net Login" $FilePath
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Successfully signed with timestamp from: $ts"
        $signed = $true
        break
    }
    Write-Warning "Timestamp server $ts failed (signtool exit $LASTEXITCODE); trying next server."
}

if (-not $signed) {
    Write-Warning "No verified timestamp was obtained; signing without a timestamp. Do not promise validity beyond certificate expiry."
    & $signtool.FullName sign /fd SHA256 /sha1 $CertificateThumbprint /d "GDOU Net Login" $FilePath
    if ($LASTEXITCODE -ne 0) { throw "signtool failed without timestamp (exit $LASTEXITCODE)." }
}

& $signtool.FullName verify /pa /all $FilePath
if ($LASTEXITCODE -ne 0) { throw "signtool verification failed (exit $LASTEXITCODE)." }
Write-Host "Verified Authenticode signature against selected certificate: $CertificateThumbprint"
