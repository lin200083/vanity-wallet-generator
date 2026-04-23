[CmdletBinding()]
param(
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount - 1),
    [int]$Seconds = 20
)

$ErrorActionPreference = "Stop"

if ($Workers -lt 1) {
    throw "Workers must be at least 1."
}

if ($Seconds -lt 5) {
    throw "Seconds must be at least 5."
}

$ProjectRoot = $PSScriptRoot
$NativeExe = Join-Path $ProjectRoot "bin\vanity-native.exe"
Set-Location $ProjectRoot

if (-not (Test-Path $NativeExe)) {
    & (Join-Path $ProjectRoot "Build-Native.ps1")
    if ($LASTEXITCODE -ne 0) {
        throw "Build-Native.ps1 failed with exit code $LASTEXITCODE."
    }
}

Write-Host "Running a $Seconds second native speed test with $Workers workers..."
Write-Host "This uses a practically unreachable suffix, so it should measure speed instead of finding a wallet."

& $NativeExe `
    --suffix "ffffffffffffffffffffffffffffffffffffffff" `
    --workers $Workers `
    --status-interval 1 `
    --max-seconds $Seconds `
    --plain-output `
    --redact-private-key `
    --state-dir "state\native-speed-test" `
    --result-dir "results\native-speed-test" `
    --logs-dir "logs"

exit $LASTEXITCODE
