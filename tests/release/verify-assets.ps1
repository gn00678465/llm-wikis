#requires -Version 5.1
<#
.SYNOPSIS
    Verifies a release asset directory's SHA256SUMS manifest.

.DESCRIPTION
    Specification §18.1; plan Task 14 Step 7: SHA256SUMS must list exactly
    five entries -- the three platform binaries plus both installers, never
    the manifest itself -- with no missing, duplicate, or unexpected entries,
    and every listed hash must match the actual file in the directory.

    Runnable standalone against any asset directory (a real release
    download, or a synthetic local fixture) with no network access.

.PARAMETER AssetDirectory
    Directory containing the release assets and SHA256SUMS.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$AssetDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fail {
    param([string]$Message)
    Write-Error "FAIL: $Message"
    exit 1
}

if (-not (Test-Path -LiteralPath $AssetDirectory -PathType Container)) {
    Fail "asset directory not found: $AssetDirectory"
}

$sumsPath = Join-Path $AssetDirectory 'SHA256SUMS'
if (-not (Test-Path -LiteralPath $sumsPath -PathType Leaf)) {
    Fail "SHA256SUMS not found at $sumsPath"
}

$expectedNames = @(
    'llm-wikis-windows-amd64.exe',
    'llm-wikis-linux-amd64',
    'llm-wikis-darwin-arm64',
    'install.sh',
    'install.ps1'
)

$lines = @(Get-Content -LiteralPath $sumsPath | Where-Object { $_.Trim() -ne '' })
if ($lines.Count -ne 5) {
    Fail "SHA256SUMS has $($lines.Count) entries, expected exactly 5"
}

$entries = [ordered]@{}
foreach ($line in $lines) {
    $parts = $line -split '\s+', 2
    if ($parts.Length -ne 2) {
        Fail "malformed SHA256SUMS line: $line"
    }
    $hash = $parts[0].ToLowerInvariant()
    $rawName = $parts[1]
    if ($rawName.StartsWith('*')) {
        Fail "SHA256SUMS contains a binary-mode '*' prefix for $($rawName.TrimStart('*')); install.sh's awk lookup would not match it"
    }
    if ($entries.Contains($rawName)) {
        Fail "duplicate filename in SHA256SUMS: $rawName"
    }
    $entries[$rawName] = $hash
}

foreach ($name in $expectedNames) {
    if (-not $entries.Contains($name)) {
        Fail "missing SHA256SUMS entry for $name"
    }
    $assetPath = Join-Path $AssetDirectory $name
    if (-not (Test-Path -LiteralPath $assetPath -PathType Leaf)) {
        Fail "expected asset file missing from directory: $name"
    }
}

foreach ($name in $entries.Keys) {
    if ($expectedNames -notcontains $name) {
        Fail "unexpected entry in SHA256SUMS: $name"
    }
}

foreach ($name in $entries.Keys) {
    $assetPath = Join-Path $AssetDirectory $name
    $actual = (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $entries[$name]) {
        Fail "hash mismatch for $name : expected $($entries[$name]), got $actual"
    }
}

$fileCount = (Get-ChildItem -LiteralPath $AssetDirectory -File).Count
if ($fileCount -ne 6) {
    Fail "asset directory contains $fileCount files, expected exactly 6 (5 assets + SHA256SUMS)"
}

Write-Host "PASS: SHA256SUMS in $AssetDirectory has exactly the 5 expected entries, all hashes match"
