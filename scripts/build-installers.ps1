param(
    [switch]$Sign
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$iss = Join-Path $root "installer\TuneItVerse.iss"
$isccCmd = Get-Command iscc.exe -ErrorAction SilentlyContinue
if ($isccCmd) {
    $iscc = $isccCmd.Source
} else {
    $inno = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
    if (Test-Path $inno) {
        $iscc = $inno
    } else {
        Write-Output "iscc not found. Install Inno Setup to build installers. Exes stay unsigned."
        exit 0
    }
}

if ($Sign -and -not $env:WINDOWS_PFX_BASE64) {
    Write-Output "Refusing to build a signed installer without WINDOWS_PFX_BASE64."
    exit 2
}

$targets = @(
    @{
        Arch = "x64"
        Paths = @(
            (Join-Path $root "src-tauri\target\x86_64-pc-windows-msvc\release\TuneItVerse.exe"),
            (Join-Path $root "src-tauri\target\release\TuneItVerse.exe")
        )
    },
    @{
        Arch = "x86"
        Paths = @(
            (Join-Path $root "src-tauri\target\i686-pc-windows-msvc\release\TuneItVerse.exe")
        )
    }
)

$built = 0
foreach ($t in $targets) {
    $exe = $null
    foreach ($p in $t.Paths) {
        if (Test-Path -LiteralPath $p) { $exe = $p; break }
    }
    if (-not $exe) {
        Write-Output ("skip {0}: exe not built" -f $t.Arch)
        continue
    }
    $payload = $exe
    $signDef = "no"
    if ($Sign) {
        $payload = Join-Path $env:TEMP ("TuneItVerse-" + $t.Arch + "-tosign.exe")
        Copy-Item -LiteralPath $exe -Destination $payload -Force
        & (Join-Path $PSScriptRoot "sign-exe.ps1") -Path $payload
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        $signDef = "yes"
    }
    $exeArg = ($payload -replace '\\', '/')
    if ($signDef -eq "yes") {
        $pfx = Join-Path $env:TEMP ("tuneitverse-inno-" + [guid]::NewGuid().ToString() + ".pfx")
        [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_PFX_BASE64))
        $pass = $env:WINDOWS_PFX_PASSWORD
        $signCmd = "signtool sign /fd SHA256 /f `"$pfx`" /p `"$pass`" /tr http://timestamp.digicert.com /td SHA256 `$f"
        & $iscc "/DArch=$($t.Arch)" "/DExePath=$exeArg" "/DSign=yes" "/Ssigntool=$signCmd" $iss
        $code = $LASTEXITCODE
        if (Test-Path -LiteralPath $pfx) { Remove-Item -LiteralPath $pfx -Force }
        if ($code -ne 0) { exit $code }
    } else {
        & $iscc "/DArch=$($t.Arch)" "/DExePath=$exeArg" "/DSign=no" $iss
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    $built++
}

if ($built -eq 0) {
    Write-Output "No exes found. Build release targets first. Nothing was signed."
    exit 0
}
Write-Output ("Built {0} installer(s). Signed={1}." -f $built, [bool]$Sign)
