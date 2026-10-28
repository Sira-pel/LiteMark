# Builds target\release\litemark.exe and wraps it in an Inno Setup installer.
# Requires Rust and Inno Setup 6 (iscc.exe on PATH or in the default install folder).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches.Groups[1].Value

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$iscc = (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    $iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { throw "Inno Setup 6 was not found. Install it from https://jrsoftware.org/isinfo.php" }

New-Item -ItemType Directory -Force build | Out-Null
& $iscc "/DAppVersion=$version" scripts\litemark.iss
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }

$setup = "build\LiteMark-$version-windows-x64-setup.exe"
(Get-FileHash $setup -Algorithm SHA256).Hash.ToLower() + "  " + (Split-Path $setup -Leaf) | Set-Content build\SHA256SUMS-windows.txt
Write-Host "Built $setup"
