# Compile Lowfreed en release avec la chaîne Rust GNU.
# windows-sys lie les DLL système en « raw-dylib » : sur la cible GNU, rustc a besoin
# de dlltool et de as, fournis par MinGW-w64 (WinLibs, installé via winget).
$ErrorActionPreference = "Stop"

$mingw = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Directory -Filter "BrechtSanders.WinLibs*" -ErrorAction SilentlyContinue |
    ForEach-Object { Join-Path $_.FullName "mingw64\bin" } |
    Where-Object { Test-Path (Join-Path $_ "dlltool.exe") } |
    Select-Object -First 1
if (-not $mingw) {
    Write-Error "MinGW-w64 introuvable : winget install BrechtSanders.WinLibs.POSIX.UCRT"
}
$env:PATH = "$env:USERPROFILE\.cargo\bin;$mingw;$env:PATH"

Set-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "OK : target\release\lowfreed.exe (interface) et target\release\lowfreed-cli.exe (ligne de commande)"
