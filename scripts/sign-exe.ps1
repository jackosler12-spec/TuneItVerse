param(
    [string]$Path,
    [switch]$CheckOnly
)

# Check-only is the CI proof that this script does not invent a signature.
if ($CheckOnly) {
    Write-Output "unsigned"
    exit 0
}

if (-not $env:WINDOWS_PFX_BASE64) {
    Write-Output "WINDOWS_PFX_BASE64 is not set. Refusing to mark the file signed."
    exit 2
}

if (-not $Path -or -not (Test-Path -LiteralPath $Path)) {
    Write-Output "Sign target is missing. Refusing to mark a file signed."
    exit 2
}

$tool = $null
$cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
if ($cmd) { $tool = $cmd.Source }
if (-not $tool) {
    $kits = "C:\Program Files (x86)\Windows Kits\10\bin"
    if (Test-Path $kits) {
        $found = Get-ChildItem -Path $kits -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($found) { $tool = $found.FullName }
    }
}
if (-not $tool) {
    Write-Output "signtool.exe was not found. Refusing to mark the file signed."
    exit 2
}

$pfx = Join-Path $env:TEMP ("tuneitverse-" + [guid]::NewGuid().ToString() + ".pfx")
try {
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_PFX_BASE64))
    $signArgs = @("sign", "/fd", "SHA256", "/f", $pfx, "/tr", "http://timestamp.digicert.com", "/td", "SHA256")
    if ($env:WINDOWS_PFX_PASSWORD) {
        $signArgs += @("/p", $env:WINDOWS_PFX_PASSWORD)
    }
    $signArgs += $Path
    & $tool @signArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Output "signed $Path"
} finally {
    if (Test-Path -LiteralPath $pfx) { Remove-Item -LiteralPath $pfx -Force }
}
