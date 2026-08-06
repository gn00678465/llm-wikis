#requires -Version 5.1
<#
.SYNOPSIS
    Verifies a release asset directory's SHA256SUMS manifest.

.DESCRIPTION
    Specification §18.1; plan Task 14 Step 7: SHA256SUMS must list exactly
    three entries -- the three platform binaries, never the manifest itself
    -- with no missing, duplicate, or unexpected entries, and every listed
    hash must match the actual file in the directory.

    The two installer scripts (install.sh, install.ps1) are deliberately NOT
    part of the release asset set: install.sh's unpinned (LLM_WIKIS_VERSION
    unset) run always resolves against .../releases/latest/download/..., so
    an installer copy attached to one specific tagged release would silently
    install `latest` instead of that release while looking self-contained --
    the release-provided SHA256SUMS entry for the installer would give no
    real integrity guarantee either, since the same release controls both
    the script and its own checksum. Both installers are fetched from
    raw.githubusercontent.com/.../refs/heads/main/ instead (see README.md /
    docs/llm-wikis.md), which is the single source of truth for them. This
    script FAILS if either installer script shows up in the asset set at
    all.

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
    'llm-wikis-darwin-arm64'
)
$forbiddenInstallerNames = @('install.sh', 'install.ps1')

$lines = @(Get-Content -LiteralPath $sumsPath | Where-Object { $_.Trim() -ne '' })
if ($lines.Count -ne 3) {
    Fail "SHA256SUMS has $($lines.Count) entries, expected exactly 3"
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
    if ($forbiddenInstallerNames -contains $rawName) {
        Fail "installer script must not be part of the release asset set: $rawName"
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

# Belt-and-braces: reject an installer script sitting in the asset directory
# even if it were somehow never listed in SHA256SUMS at all.
foreach ($name in $forbiddenInstallerNames) {
    $installerPath = Join-Path $AssetDirectory $name
    if (Test-Path -LiteralPath $installerPath -PathType Leaf) {
        Fail "installer script must not be part of the release asset directory: $name"
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
if ($fileCount -ne 4) {
    Fail "asset directory contains $fileCount files, expected exactly 4 (3 assets + SHA256SUMS)"
}

Write-Host "PASS: SHA256SUMS in $AssetDirectory has exactly the 3 expected entries, all hashes match"
