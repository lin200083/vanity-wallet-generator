[CmdletBinding()]
param(
    [string]$Prefix = "",
    [string]$Suffix = "00000000",
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount - 1),
    [int]$StatusIntervalSeconds = 5,
    [int]$BatchSize = 1024,
    [int]$MaxSeconds = 0,
    [switch]$CaseSensitive,
    [switch]$PreventSleep,
    [switch]$NoBuild,
    [switch]$RedactPrivateKey,
    [switch]$PlainOutput
)

$ErrorActionPreference = "Stop"

function Normalize-HexPattern {
    param(
        [string]$Value,
        [string]$Name
    )

    $normalized = $Value.Trim()
    if ($normalized.StartsWith("0x", [System.StringComparison]::OrdinalIgnoreCase)) {
        $normalized = $normalized.Substring(2)
    }

    if ($normalized -notmatch '^[0-9a-fA-F]*$') {
        throw "$Name must contain only hexadecimal characters, optionally prefixed by 0x."
    }

    return $normalized
}

function Enable-PreventSystemSleep {
    if (-not ([System.Management.Automation.PSTypeName]'VanityWalletSleepUtil').Type) {
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class VanityWalletSleepUtil
{
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern uint SetThreadExecutionState(uint esFlags);
}
"@
    }

    $ES_CONTINUOUS = [uint32]2147483648
    $ES_SYSTEM_REQUIRED = [uint32]1
    [VanityWalletSleepUtil]::SetThreadExecutionState($ES_CONTINUOUS -bor $ES_SYSTEM_REQUIRED) | Out-Null
}

function Disable-PreventSystemSleep {
    if (([System.Management.Automation.PSTypeName]'VanityWalletSleepUtil').Type) {
        $ES_CONTINUOUS = [uint32]2147483648
        [VanityWalletSleepUtil]::SetThreadExecutionState($ES_CONTINUOUS) | Out-Null
    }
}

$Prefix = Normalize-HexPattern -Value $Prefix -Name "Prefix"
$Suffix = Normalize-HexPattern -Value $Suffix -Name "Suffix"

if (($Prefix.Length + $Suffix.Length) -eq 0) {
    throw "At least one of Prefix or Suffix must be provided."
}

if (($Prefix.Length + $Suffix.Length) -gt 40) {
    throw "Prefix plus suffix cannot exceed 40 hex characters for an EVM address."
}

if ($Workers -lt 1) {
    throw "Workers must be at least 1."
}

if ($StatusIntervalSeconds -lt 1) {
    throw "StatusIntervalSeconds must be at least 1."
}

if ($BatchSize -lt 1) {
    throw "BatchSize must be at least 1."
}

if ($MaxSeconds -lt 0) {
    throw "MaxSeconds cannot be negative."
}

$ProjectRoot = $PSScriptRoot
$NativeExe = Join-Path $ProjectRoot "bin\vanity-native.exe"
Set-Location $ProjectRoot

if ((-not (Test-Path $NativeExe)) -and (-not $NoBuild)) {
    & (Join-Path $ProjectRoot "Build-Native.ps1")
    if ($LASTEXITCODE -ne 0) {
        throw "Build-Native.ps1 failed with exit code $LASTEXITCODE."
    }
}

if (-not (Test-Path $NativeExe)) {
    throw "Native executable was not found at $NativeExe. Run .\Build-Native.ps1 first, or run without -NoBuild."
}

$nativeArgs = @(
    "--workers", $Workers.ToString(),
    "--status-interval", $StatusIntervalSeconds.ToString(),
    "--batch-size", $BatchSize.ToString()
)

if ($Prefix.Length -gt 0) {
    $nativeArgs += @("--prefix", $Prefix)
}

if ($Suffix.Length -gt 0) {
    $nativeArgs += @("--suffix", $Suffix)
}

if ($MaxSeconds -gt 0) {
    $nativeArgs += @("--max-seconds", $MaxSeconds.ToString())
}

if ($CaseSensitive) {
    $nativeArgs += "--case-sensitive"
}

if ($RedactPrivateKey) {
    $nativeArgs += "--redact-private-key"
}

if ($PlainOutput) {
    $nativeArgs += "--plain-output"
}

Write-Host "Starting native EVM vanity search..."
Write-Host "Prefix: '$Prefix'  Suffix: '$Suffix'  Workers: $Workers"
Write-Host "Press Ctrl+C to stop. Results are written under: $ProjectRoot\results"

try {
    if ($PreventSleep) {
        Enable-PreventSystemSleep
        Write-Host "System sleep prevention is enabled for this PowerShell process."
    }

    & $NativeExe @nativeArgs
    exit $LASTEXITCODE
}
finally {
    if ($PreventSleep) {
        Disable-PreventSystemSleep
    }
}
