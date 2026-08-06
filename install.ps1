#requires -Version 5.1
<#
.SYNOPSIS
    llm-wikis installer for Windows x64.

.DESCRIPTION
    Downloads the release asset matching this machine's architecture,
    verifies it against SHA256SUMS, smoke-tests it with --version, installs
    it to %LOCALAPPDATA%\llm-wikis\bin\llm-wikis.exe, and adds that directory
    to the user PATH exactly once.

    Production always targets the fixed GitHub repository below and always
    writes the real user-scope PATH. Every LLM_WIKIS_TEST_* override below is
    inert unless LLM_WIKIS_INSTALLER_TEST=1 is also set, so a real end-user
    run can never be redirected to another origin or PATH-write target.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Write-Info {
    param([string]$Message)
    Write-Host $Message
}

function Invoke-Fail {
    param([string]$Message)
    Write-Error "error: $Message"
    exit 1
}

$Repo = 'gn00678465/llm-wikis'
$BaseUrl = "https://github.com/$Repo/releases"
$InstallRoot = Join-Path $env:LOCALAPPDATA 'llm-wikis'
$Arch = $env:PROCESSOR_ARCHITECTURE
$UseSandboxPath = $false
$PathSandboxFile = $null

if ($env:LLM_WIKIS_INSTALLER_TEST -eq '1') {
    if ($env:LLM_WIKIS_TEST_BASE_URL) { $BaseUrl = $env:LLM_WIKIS_TEST_BASE_URL }
    if ($env:LLM_WIKIS_TEST_INSTALL_ROOT) { $InstallRoot = $env:LLM_WIKIS_TEST_INSTALL_ROOT }
    if ($env:LLM_WIKIS_TEST_ARCH) { $Arch = $env:LLM_WIKIS_TEST_ARCH }
    if ($env:LLM_WIKIS_TEST_PATH_FILE) {
        $UseSandboxPath = $true
        $PathSandboxFile = $env:LLM_WIKIS_TEST_PATH_FILE
    }
}

if ($Arch -ne 'AMD64') {
    Invoke-Fail "unsupported architecture: $Arch (llm-wikis supports Windows x64 only)"
}
$Asset = 'llm-wikis-windows-amd64.exe'

$InstallDir = Join-Path $InstallRoot 'bin'
$BinaryPath = Join-Path $InstallDir 'llm-wikis.exe'

$Version = $env:LLM_WIKIS_VERSION
if ($Version) {
    $DownloadBase = "$BaseUrl/download/$Version"
} else {
    $DownloadBase = "$BaseUrl/latest/download"
}

# GitHub's .../releases/latest/download/... deliberately skips pre-releases,
# so an unpinned run 404s whenever only pre-releases have been published
# (e.g. during a beta-only period). Rather than a bare download failure,
# tell the operator exactly what to do about it -- no GitHub API call here,
# since that would add unauthenticated rate limits and fragile JSON parsing.
$NoStableReleaseMsg = "no stable release published yet -- set LLM_WIKIS_VERSION to a specific tag (see https://github.com/$Repo/releases)"

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("llm-wikis-install-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null

try {
    $AssetPath = Join-Path $TmpDir $Asset
    $ChecksumPath = Join-Path $TmpDir 'SHA256SUMS'

    Write-Info "Downloading $Asset..."
    try {
        Invoke-WebRequest -Uri "$DownloadBase/$Asset" -OutFile $AssetPath -UseBasicParsing
        Invoke-WebRequest -Uri "$DownloadBase/SHA256SUMS" -OutFile $ChecksumPath -UseBasicParsing
    } catch {
        if (-not $Version) {
            Invoke-Fail $NoStableReleaseMsg
        } else {
            Invoke-Fail "download failed: $($_.Exception.Message)"
        }
    }

    $expected = $null
    foreach ($line in Get-Content -LiteralPath $ChecksumPath) {
        $parts = $line -split '\s+', 2
        if ($parts.Length -eq 2 -and $parts[1].TrimStart('*') -eq $Asset) {
            $expected = $parts[0].ToLowerInvariant()
            break
        }
    }
    if (-not $expected) {
        Invoke-Fail "no checksum entry for $Asset in SHA256SUMS"
    }

    $actual = (Get-FileHash -LiteralPath $AssetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        Invoke-Fail "checksum mismatch for $Asset : expected $expected, got $actual"
    }

    $smokeExit = 1
    try {
        & $AssetPath --version | Out-Null
        $smokeExit = $LASTEXITCODE
    } catch {
        $smokeExit = 1
    }
    if ($smokeExit -ne 0) {
        Invoke-Fail "downloaded binary failed smoke test (--version)"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -LiteralPath $AssetPath -Destination $BinaryPath -Force

    Write-Info "Installed llm-wikis to $BinaryPath"

    # Read/write the raw, unexpanded PATH string only (no
    # [Environment]::ExpandEnvironmentVariables anywhere in this block) so an
    # unrelated %VAR%-style entry belonging to another tool is never expanded
    # and rewritten out from under it.
    if ($UseSandboxPath) {
        $userPath = if (Test-Path -LiteralPath $PathSandboxFile) { Get-Content -LiteralPath $PathSandboxFile -Raw } else { '' }
    } else {
        $userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    }
    if ($null -eq $userPath) { $userPath = '' }

    $entries = $userPath -split ';' | Where-Object { $_ -ne '' }
    $already = $entries | Where-Object { $_.TrimEnd('\') -ieq $InstallDir.TrimEnd('\') }

    if (-not $already) {
        if ($userPath -and -not $userPath.EndsWith(';')) {
            $newPath = "$userPath;$InstallDir"
        } else {
            $newPath = "$userPath$InstallDir"
        }
        if ($UseSandboxPath) {
            Set-Content -LiteralPath $PathSandboxFile -Value $newPath -NoNewline
        } else {
            [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        }
        Write-Info "Added $InstallDir to your user PATH. Restart your terminal to pick it up."
    }

    & $BinaryPath --version
}
finally {
    Remove-Item -LiteralPath $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
