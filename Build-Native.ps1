[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$ProjectRoot = $PSScriptRoot
$ManifestPath = Join-Path $ProjectRoot "native\vanity-native\Cargo.toml"
$ReleaseExe = Join-Path $ProjectRoot "native\vanity-native\target\release\vanity-native.exe"
$BinDir = Join-Path $ProjectRoot "bin"
$BinExe = Join-Path $BinDir "vanity-native.exe"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Cargo/Rust was not found in PATH. Install Rust, then try again."
}

Write-Host "Building native Rust executable..."
& cargo build --release --manifest-path $ManifestPath
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE."
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item -Force -Path $ReleaseExe -Destination $BinExe

Write-Host "Native executable ready:"
Write-Host $BinExe
