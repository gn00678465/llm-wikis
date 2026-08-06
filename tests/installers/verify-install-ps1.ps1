#requires -Version 5.1
<#
.SYNOPSIS
    Offline contract tests for install.ps1 (Windows verify script, INST-01..06/15/16/29).

.DESCRIPTION
    Runs install.ps1 against a local, loopback-only fake release server (a
    System.Net.HttpListener bound to 127.0.0.1) serving a fixture directory
    built from the real target\debug\llm-wikis.exe (renamed) plus a copy of
    pwsh.exe standing in for a second, distinguishable "version". No public
    network request is made.

    Safety: every install.ps1 invocation here sets LLM_WIKIS_INSTALLER_TEST=1
    together with LLM_WIKIS_TEST_INSTALL_ROOT (a temp directory) and
    LLM_WIKIS_TEST_PATH_FILE (a plain text file standing in for the user PATH
    registry value). install.ps1 only calls
    [Environment]::SetEnvironmentVariable('PATH', ..., 'User') when
    LLM_WIKIS_TEST_PATH_FILE is NOT set, so this script never touches the
    real user PATH registry value. Each install.ps1 run is a genuine child
    pwsh.exe process (via System.Diagnostics.Process), so its internal `exit`
    calls never terminate this harness.

    Exit code: 0 if every assertion passed, 1 if any failed.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$InstallScript = Join-Path $RepoRoot 'install.ps1'
if (-not (Test-Path -LiteralPath $InstallScript)) {
    throw "install.ps1 not found at $InstallScript"
}

$script:PassCount = 0
$script:FailCount = 0
$script:Skipped = @()

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if ($Condition) {
        $script:PassCount++
        Write-Host "  PASS: $Message"
    } else {
        $script:FailCount++
        Write-Host "  FAIL: $Message" -ForegroundColor Red
    }
}

function Skip-Row {
    param([string]$Message, [string]$Reason)
    $script:Skipped += "$Message -- $Reason"
    Write-Host "  SKIP: $Message -- $Reason" -ForegroundColor Yellow
}

# --- fixture root -----------------------------------------------------

$FixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("llm-wikis-verify-fixture-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $FixtureRoot -Force | Out-Null

$RealBinary = Join-Path $RepoRoot 'target\debug\llm-wikis.exe'
if (-not (Test-Path -LiteralPath $RealBinary)) {
    throw "target\debug\llm-wikis.exe not found; build it first (cargo build)"
}
$PwshExe = (Get-Process -Id $PID).Path

# "Version 2" is the real binary with marker bytes appended after its PE image.
# Windows' PE loader ignores trailing data past the last section, so this still
# executes identically (confirmed: still exits 0, still prints "llm-wikis
# 0.1.0") while giving a genuinely different SHA-256 hash -- enough to prove an
# install actually replaced the file's *content*, without needing a second,
# independently-behaving real executable (a raw copy of pwsh.exe was tried
# first and rejected: it is a dotnet apphost that requires sibling files
# copied alongside it, so it fails to launch once alone in a fixture
# directory -- exactly the real red failure this Step 3 run is required to
# surface; see the checkpoint notes).
$V2Binary = Join-Path $FixtureRoot 'v2-source.exe'
Copy-Item -LiteralPath $RealBinary -Destination $V2Binary -Force
[IO.File]::AppendAllText($V2Binary, 'llm-wikis-installer-verify-v2-marker-0000000000000000000000000000')

$script:FixtureHashes = @{}

function New-ReleaseDir {
    param([string]$RelativePath, [string]$SourceBinary, [bool]$CorruptChecksum = $false)
    $dir = Join-Path $FixtureRoot $RelativePath
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    $assetPath = Join-Path $dir 'llm-wikis-windows-amd64.exe'
    Copy-Item -LiteralPath $SourceBinary -Destination $assetPath -Force
    $hash = (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $script:FixtureHashes[$RelativePath] = $hash
    if ($CorruptChecksum) {
        $hash = ('0' * 64)
    }
    Set-Content -LiteralPath (Join-Path $dir 'SHA256SUMS') -Value "$hash  llm-wikis-windows-amd64.exe`n" -NoNewline
    return $assetPath
}

# v1 / "latest": the real llm-wikis binary -> --version prints "llm-wikis 0.1.0"
New-ReleaseDir -RelativePath 'latest\download' -SourceBinary $RealBinary | Out-Null
New-ReleaseDir -RelativePath 'download\v1.0.0' -SourceBinary $RealBinary | Out-Null
# v2: real binary + trailing marker bytes -> distinguishable content hash, identical --version text
New-ReleaseDir -RelativePath 'download\v2.0.0' -SourceBinary $V2Binary | Out-Null
# checksum-mismatch fixture: real binary content, deliberately wrong SHA256SUMS entry
New-ReleaseDir -RelativePath 'download\bad-checksum' -SourceBinary $RealBinary -CorruptChecksum $true | Out-Null
# smoke-failure fixture: garbage bytes named as the asset, matching (correct) checksum for
# that garbage, so the checksum gate passes and only the --version smoke test fails.
$smokeDir = Join-Path $FixtureRoot 'download\smoke-fail'
New-Item -ItemType Directory -Path $smokeDir -Force | Out-Null
$smokeAsset = Join-Path $smokeDir 'llm-wikis-windows-amd64.exe'
[IO.File]::WriteAllBytes($smokeAsset, [byte[]](1..64))
$smokeHash = (Get-FileHash -LiteralPath $smokeAsset -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $smokeDir 'SHA256SUMS') -Value "$smokeHash  llm-wikis-windows-amd64.exe`n" -NoNewline

# --- loopback fixture HTTP server --------------------------------------

$Port = Get-Random -Minimum 20000 -Maximum 60000
$Prefix = "http://127.0.0.1:$Port/"
$Listener = New-Object System.Net.HttpListener
$Listener.Prefixes.Add($Prefix)
$Listener.Start()

$ServerJob = Start-ThreadJob -ScriptBlock {
    param($Listener, $Root)
    while ($Listener.IsListening) {
        try {
            $context = $Listener.GetContext()
        } catch {
            break
        }
        try {
            $reqPath = [Uri]::UnescapeDataString($context.Request.Url.AbsolutePath).TrimStart('/')
            $filePath = Join-Path $Root ($reqPath -replace '/', [IO.Path]::DirectorySeparatorChar)
            if (Test-Path -LiteralPath $filePath -PathType Leaf) {
                $bytes = [IO.File]::ReadAllBytes($filePath)
                $context.Response.StatusCode = 200
                $context.Response.ContentLength64 = $bytes.Length
                $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
            } else {
                $context.Response.StatusCode = 404
            }
        } catch {
            # best-effort; a broken pipe here just fails the calling test assertion
        } finally {
            $context.Response.OutputStream.Close()
        }
    }
} -ArgumentList $Listener, $FixtureRoot

Start-Sleep -Milliseconds 200 # let the listener thread reach GetContext() before first request

# --- installer invocation helper ---------------------------------------

function Invoke-Installer {
    param([hashtable]$EnvOverrides = @{})
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $PwshExe
    foreach ($a in @('-NoProfile', '-NonInteractive', '-File', $InstallScript)) {
        $psi.ArgumentList.Add($a)
    }
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    foreach ($k in $EnvOverrides.Keys) {
        $psi.Environment[$k] = $EnvOverrides[$k]
    }
    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()
    [PSCustomObject]@{
        ExitCode = $proc.ExitCode
        StdOut   = $stdout
        StdErr   = $stderr
    }
}

$script:SandboxRoots = @()
$script:SandboxPathFiles = @()

function New-Sandbox {
    # Fresh, isolated install root + PATH-sandbox file for one test case.
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("llm-wikis-verify-root-" + [System.Guid]::NewGuid().ToString('N'))
    $pathFile = Join-Path ([System.IO.Path]::GetTempPath()) ("llm-wikis-verify-pathfile-" + [System.Guid]::NewGuid().ToString('N') + '.txt')
    $script:SandboxRoots += $root
    $script:SandboxPathFiles += $pathFile
    [PSCustomObject]@{
        InstallRoot = $root
        PathFile    = $pathFile
        BinaryPath  = Join-Path $root 'bin\llm-wikis.exe'
    }
}

function Base-Env {
    param($Sandbox)
    @{
        LLM_WIKIS_INSTALLER_TEST  = '1'
        LLM_WIKIS_TEST_BASE_URL   = "http://127.0.0.1:$Port"
        LLM_WIKIS_TEST_INSTALL_ROOT = $Sandbox.InstallRoot
        LLM_WIKIS_TEST_PATH_FILE  = $Sandbox.PathFile
        LLM_WIKIS_TEST_ARCH       = 'AMD64'
    }
}

try {
    Write-Host "=== INST-01: latest resolves to the newest fixture (no LLM_WIKIS_VERSION) ==="
    $sb = New-Sandbox
    $env1 = Base-Env $sb
    $r = Invoke-Installer -EnvOverrides $env1
    Assert-True ($r.ExitCode -eq 0) "latest install exits 0 (stderr: $($r.StdErr))"
    Assert-True (Test-Path -LiteralPath $sb.BinaryPath) "latest install places llm-wikis.exe at $($sb.BinaryPath)"
    Assert-True ($r.StdOut -match 'llm-wikis 0\.1\.0') "latest install's final --version prints llm-wikis 0.1.0"

    Write-Host "=== INST-01: pinned LLM_WIKIS_VERSION resolves to that exact tag ==="
    $sb = New-Sandbox
    $env2 = Base-Env $sb
    $env2['LLM_WIKIS_VERSION'] = 'v2.0.0'
    $r = Invoke-Installer -EnvOverrides $env2
    Assert-True ($r.ExitCode -eq 0) "pinned v2.0.0 install exits 0 (stderr: $($r.StdErr))"
    $installedHash = if (Test-Path -LiteralPath $sb.BinaryPath) { (Get-FileHash -LiteralPath $sb.BinaryPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
    Assert-True ($installedHash -eq $script:FixtureHashes['download\v2.0.0']) "pinned v2.0.0 install's installed-binary hash matches the v2.0.0 fixture asset's hash exactly (proves it fetched v2.0.0, not v1.0.0/latest), and differs from the v1.0.0 fixture's hash ($($script:FixtureHashes['download\v1.0.0']))"

    Write-Host "=== INST-02: unsupported architecture errors explicitly, before any download ==="
    $sb = New-Sandbox
    $envArch = Base-Env $sb
    $envArch['LLM_WIKIS_TEST_ARCH'] = 'ARM64'
    $r = Invoke-Installer -EnvOverrides $envArch
    Assert-True ($r.ExitCode -ne 0) "ARM64 install exits non-zero"
    Assert-True ($r.StdErr -match 'unsupported architecture') "ARM64 install prints an explicit unsupported-architecture error"
    Assert-True (-not (Test-Path -LiteralPath $sb.BinaryPath)) "ARM64 install places no binary"

    Write-Host "=== INST-03/INST-29: checksum mismatch fails closed ==="
    $sb = New-Sandbox
    $envBad = Base-Env $sb
    $envBad['LLM_WIKIS_VERSION'] = 'bad-checksum'
    $r = Invoke-Installer -EnvOverrides $envBad
    Assert-True ($r.ExitCode -ne 0) "checksum-mismatch install exits non-zero"
    Assert-True ($r.StdErr -match 'checksum mismatch') "checksum-mismatch install prints an explicit checksum-mismatch error"
    Assert-True (-not (Test-Path -LiteralPath $sb.BinaryPath)) "checksum-mismatch install places no binary (fail-closed)"

    Write-Host "=== INST-04: smoke-test failure (--version fails) aborts before install ==="
    $sb = New-Sandbox
    $envSmoke = Base-Env $sb
    $envSmoke['LLM_WIKIS_VERSION'] = 'smoke-fail'
    $r = Invoke-Installer -EnvOverrides $envSmoke
    Assert-True ($r.ExitCode -ne 0) "smoke-failure install exits non-zero"
    Assert-True ($r.StdErr -match 'smoke test') "smoke-failure install reports the --version smoke-test failure"
    Assert-True (-not (Test-Path -LiteralPath $sb.BinaryPath)) "smoke-failure install places no binary"

    Write-Host "=== INST-05: successful install lands at exactly %LOCALAPPDATA%\llm-wikis\bin\llm-wikis.exe (sandboxed root) ==="
    $sb = New-Sandbox
    $r = Invoke-Installer -EnvOverrides (Base-Env $sb)
    $expectedPath = Join-Path $sb.InstallRoot 'bin\llm-wikis.exe'
    Assert-True ($r.ExitCode -eq 0 -and (Test-Path -LiteralPath $expectedPath)) "binary present at exactly <install-root>\bin\llm-wikis.exe"

    Write-Host "=== INST-06: user PATH gains the bin dir exactly once across two runs ==="
    $sb = New-Sandbox
    $envPath = Base-Env $sb
    $r1 = Invoke-Installer -EnvOverrides $envPath
    Assert-True ($r1.ExitCode -eq 0) "first PATH-idempotency run exits 0"
    $content1 = Get-Content -LiteralPath $sb.PathFile -Raw
    $binDir = Join-Path $sb.InstallRoot 'bin'
    $count1 = ([regex]::Matches($content1, [regex]::Escape($binDir))).Count
    Assert-True ($count1 -eq 1) "sandbox PATH file contains the bin dir exactly once after run 1 (got $count1): '$content1'"
    $r2 = Invoke-Installer -EnvOverrides $envPath
    Assert-True ($r2.ExitCode -eq 0) "second PATH-idempotency run exits 0"
    $content2 = Get-Content -LiteralPath $sb.PathFile -Raw
    $count2 = ([regex]::Matches($content2, [regex]::Escape($binDir))).Count
    Assert-True ($count2 -eq 1) "sandbox PATH file still contains the bin dir exactly once after run 2, no duplicate added (got $count2): '$content2'"

    Write-Host "=== INST-15: temp directory is cleaned up on success and on failure ==="
    $tempsBefore = Get-ChildItem -Path ([System.IO.Path]::GetTempPath()) -Filter 'llm-wikis-install-*' -Directory -ErrorAction SilentlyContinue
    $sb = New-Sandbox
    Invoke-Installer -EnvOverrides (Base-Env $sb) | Out-Null # success path
    $sbFail = New-Sandbox
    $envFail = Base-Env $sbFail
    $envFail['LLM_WIKIS_VERSION'] = 'bad-checksum'
    Invoke-Installer -EnvOverrides $envFail | Out-Null # failure path
    Start-Sleep -Milliseconds 200
    $tempsAfter = Get-ChildItem -Path ([System.IO.Path]::GetTempPath()) -Filter 'llm-wikis-install-*' -Directory -ErrorAction SilentlyContinue
    Assert-True (($tempsAfter | Measure-Object).Count -eq ($tempsBefore | Measure-Object).Count) "no leftover llm-wikis-install-* temp directory after either a successful or a failed run"

    Write-Host "=== INST-16: reinstall upgrades, downgrades, and repairs the binary ==="
    $sb = New-Sandbox
    $envA = Base-Env $sb; $envA['LLM_WIKIS_VERSION'] = 'v1.0.0'
    $envB = Base-Env $sb; $envB['LLM_WIKIS_VERSION'] = 'v2.0.0'
    $hashV1 = $script:FixtureHashes['download\v1.0.0']
    $hashV2 = $script:FixtureHashes['download\v2.0.0']
    function Get-InstalledHash($Sandbox) {
        if (Test-Path -LiteralPath $Sandbox.BinaryPath) { (Get-FileHash -LiteralPath $Sandbox.BinaryPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
    }
    $rA1 = Invoke-Installer -EnvOverrides $envA
    Assert-True ($rA1.ExitCode -eq 0 -and (Get-InstalledHash $sb) -eq $hashV1) "pinned v1.0.0 install's installed binary matches the v1.0.0 fixture hash"
    $rB = Invoke-Installer -EnvOverrides $envB
    Assert-True ($rB.ExitCode -eq 0 -and (Get-InstalledHash $sb) -eq $hashV2) "re-running pinned to v2.0.0 upgrades in place: installed binary now matches the v2.0.0 fixture hash (differs from v1.0.0's $hashV1)"
    $rA2 = Invoke-Installer -EnvOverrides $envA
    Assert-True ($rA2.ExitCode -eq 0 -and (Get-InstalledHash $sb) -eq $hashV1) "re-running pinned back to v1.0.0 downgrades in place: installed binary matches the v1.0.0 fixture hash again"

    Write-Host "=== defect-2b: unpinned 'latest' install fails with a clear, actionable message when no stable release exists ==="
    # Separate loopback server bound to an empty directory -- no 'latest'
    # release published at all -- so /latest/download/... 404s exactly like
    # a real repository with only pre-releases published (GitHub's
    # releases/latest/download/... deliberately skips pre-releases).
    $NoLatestRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("llm-wikis-verify-nolatest-" + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $NoLatestRoot -Force | Out-Null
    $NoLatestPort = Get-Random -Minimum 20000 -Maximum 60000
    $NoLatestListener = New-Object System.Net.HttpListener
    $NoLatestListener.Prefixes.Add("http://127.0.0.1:$NoLatestPort/")
    $NoLatestListener.Start()
    $NoLatestJob = Start-ThreadJob -ScriptBlock {
        param($Listener)
        while ($Listener.IsListening) {
            try {
                $context = $Listener.GetContext()
            } catch {
                break
            }
            try {
                $context.Response.StatusCode = 404
            } catch {
            } finally {
                $context.Response.OutputStream.Close()
            }
        }
    } -ArgumentList $NoLatestListener
    Start-Sleep -Milliseconds 200
    try {
        $sb = New-Sandbox
        $envNoLatest = @{
            LLM_WIKIS_INSTALLER_TEST    = '1'
            LLM_WIKIS_TEST_BASE_URL     = "http://127.0.0.1:$NoLatestPort"
            LLM_WIKIS_TEST_INSTALL_ROOT = $sb.InstallRoot
            LLM_WIKIS_TEST_PATH_FILE    = $sb.PathFile
            LLM_WIKIS_TEST_ARCH         = 'AMD64'
        }
        $r = Invoke-Installer -EnvOverrides $envNoLatest
        Assert-True ($r.ExitCode -ne 0) "unpinned install against a repository with no stable release exits non-zero"
        Assert-True ($r.StdErr -match 'no stable release published yet') "unpinned install against a repository with no stable release prints the explicit actionable error, not a bare download failure"
        Assert-True (-not (Test-Path -LiteralPath $sb.BinaryPath)) "unpinned install against a repository with no stable release places no binary"
    } finally {
        try { $NoLatestListener.Stop() } catch {}
        try { $NoLatestListener.Close() } catch {}
        Wait-Job -Job $NoLatestJob -Timeout 5 | Out-Null
        Remove-Job -Job $NoLatestJob -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $NoLatestRoot -Recurse -Force -ErrorAction SilentlyContinue
    }

    Write-Host "=== INST-10 (missing checksum tool): not applicable to install.ps1 ==="
    Skip-Row "INST-10 missing-checksum-tool case" "Get-FileHash is a built-in PowerShell cmdlet, not an external tool that can be absent the way sha256sum/shasum can on POSIX; this row is scoped to install.sh only (platform column: Linux/WSL, macOS)."

    Write-Host "=== INST-13/14 (POSIX profile idempotency / unsupported shell): not applicable to install.ps1 ==="
    Skip-Row "INST-13/INST-14 POSIX profile cases" "install.ps1 has no shell-profile concept; the Windows PATH-idempotency equivalent is INST-06, covered above. These rows belong to install.sh, exercised by tests/installers/verify-install-sh.sh (PENDING on this Windows host per the task's platform-ownership rule)."
}
finally {
    try { $Listener.Stop() } catch {}
    try { $Listener.Close() } catch {}
    Wait-Job -Job $ServerJob -Timeout 5 | Out-Null
    Remove-Job -Job $ServerJob -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $FixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
    foreach ($root in $script:SandboxRoots) {
        Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
    }
    foreach ($f in $script:SandboxPathFiles) {
        Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue
    }
}

Write-Host ""
Write-Host "=== Summary: $script:PassCount passed, $script:FailCount failed, $($script:Skipped.Count) skipped ==="
if ($script:Skipped.Count -gt 0) {
    Write-Host "Skipped:"
    $script:Skipped | ForEach-Object { Write-Host "  - $_" }
}

if ($script:FailCount -gt 0) {
    exit 1
}
exit 0
