[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$StatusPath = Join-Path $PSScriptRoot "state\status.json"

if (-not (Test-Path $StatusPath)) {
    Write-Host "No status file found yet."
    exit 0
}

$status = Get-Content -Raw -Path $StatusPath | ConvertFrom-Json

Write-Host "Run ID:        $($status.runId)"
if ($status.PSObject.Properties.Name -contains "engine") {
    Write-Host "Engine:        $($status.engine)"
}
Write-Host "Target:        prefix '$($status.pattern.prefix)' suffix '$($status.pattern.suffix)'"
Write-Host "Attempts:      $($status.totalAttempts)"
Write-Host "Rate:          $($status.totalRatePerSecond) / sec"
Write-Host "Runtime:       $($status.runtime)"
Write-Host "Workers:       $($status.aliveWorkers) / $($status.configuredWorkers)"
Write-Host "Restarts:      $($status.totalRestarts)"
Write-Host "Matched:       $($status.matched)"
Write-Host "Last updated:  $($status.updatedAt)"
