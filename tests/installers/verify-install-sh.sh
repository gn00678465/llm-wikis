#!/bin/sh
# Offline contract tests for install.sh.
#
# AUTHORED on a Windows implementation host and NOT executed there (see
# docs/verification/llm-wikis-execution.md, Task 13 checkpoint, worker
# task13-installer-worker-2026-08-01): the corresponding INST checklist rows
# stay PENDING until this runs natively on Linux or macOS (Task 14 CI, or a
# manual run on that platform):
#
#   sh tests/installers/verify-install-sh.sh
#
# Do not run this through WSL or another Windows emulation layer and report
# the result as native Linux/macOS coverage -- that would violate the plan's
# platform-ownership rule (each platform runs only its own verify script).
#
# Safety: every install.sh invocation below sets LLM_WIKIS_INSTALLER_TEST=1
# together with a fake LLM_WIKIS_TEST_HOME (so ~/.zprofile / ~/.profile
# writes land in a throwaway temp directory, never the real operator HOME),
# a fake LLM_WIKIS_TEST_INSTALL_DIR, a per-test TMPDIR sandbox, and a
# file://-served local fixture tree (built from small POSIX-shell stand-in
# "binaries", never a public download). Nothing under the real $HOME,
# ~/.local/bin, ~/.profile, or ~/.zprofile is ever touched by this script.
set -eu

REPO_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
INSTALL_SCRIPT="${REPO_ROOT}/install.sh"
if [ ! -f "$INSTALL_SCRIPT" ]; then
  echo "install.sh not found at $INSTALL_SCRIPT" >&2
  exit 1
fi

PASS_COUNT=0
FAIL_COUNT=0
SKIP_NOTES=""

pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf '  PASS: %s\n' "$1"
}

fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '  FAIL: %s\n' "$1"
}

skip() {
  SKIP_NOTES="${SKIP_NOTES}
  - $1 -- $2"
  printf '  SKIP: %s -- %s\n' "$1" "$2"
}

WORK="$(mktemp -d)"
cleanup_harness() {
  rm -rf "$WORK"
}
trap cleanup_harness EXIT INT TERM

# --- fixture tree, served via file:// (no network, no HTTP listener needed) ---

FIXTURE_ROOT="${WORK}/fixture"
mkdir -p "$FIXTURE_ROOT"

if command -v sha256sum >/dev/null 2>&1; then
  hash_of() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  hash_of() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  echo "this harness needs sha256sum or shasum to build fixtures" >&2
  exit 1
fi

# add_asset <relative-dir-under-fixture-root> <asset-name> <version-text> <exit-code> [bad]
# Writes (or appends to) a shared SHA256SUMS in that directory, so a Linux
# and a macOS asset can coexist under the same "latest"/tag slot exactly as
# a real GitHub release does.
add_asset() {
  rel="$1"; asset_name="$2"; version_text="$3"; exit_code="$4"; bad="${5:-}"
  dir="${FIXTURE_ROOT}/${rel}"
  mkdir -p "$dir"
  asset_path="${dir}/${asset_name}"
  cat > "$asset_path" <<SCRIPT
#!/bin/sh
echo "$version_text"
exit $exit_code
SCRIPT
  chmod +x "$asset_path"
  h="$(hash_of "$asset_path")"
  if [ -n "$bad" ]; then
    h="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcd"
  fi
  printf '%s  %s\n' "$h" "$asset_name" >> "${dir}/SHA256SUMS"
}

add_asset "latest/download" "llm-wikis-linux-amd64" "llm-wikis 0.1.0" 0
add_asset "latest/download" "llm-wikis-darwin-arm64" "llm-wikis 0.1.0" 0
add_asset "download/v1.0.0" "llm-wikis-linux-amd64" "llm-wikis 0.1.0" 0
add_asset "download/v1.0.0" "llm-wikis-darwin-arm64" "llm-wikis 0.1.0" 0
add_asset "download/v2.0.0" "llm-wikis-linux-amd64" "llm-wikis 0.2.0-fixture" 0
add_asset "download/v2.0.0" "llm-wikis-darwin-arm64" "llm-wikis 0.2.0-fixture" 0
add_asset "download/bad-checksum" "llm-wikis-linux-amd64" "llm-wikis 0.1.0" 0 bad
add_asset "download/bad-checksum" "llm-wikis-darwin-arm64" "llm-wikis 0.1.0" 0 bad
add_asset "download/smoke-fail" "llm-wikis-linux-amd64" "llm-wikis 0.1.0" 1
add_asset "download/smoke-fail" "llm-wikis-darwin-arm64" "llm-wikis 0.1.0" 1

BASE_URL="file://${FIXTURE_ROOT}"

# --- installer invocation helper ---------------------------------------

# run_installer KEY=VALUE ...
# Runs install.sh as a genuine child `sh` process via `env`, so its `exit`
# calls never terminate this harness. Sets EXIT_CODE/STDOUT_TEXT/STDERR_TEXT.
run_installer() {
  stdout_file="${WORK}/stdout.$$.$RANDOM_ID"
  stderr_file="${WORK}/stderr.$$.$RANDOM_ID"
  RANDOM_ID=$((RANDOM_ID + 1))
  set +e
  env "$@" sh "$INSTALL_SCRIPT" >"$stdout_file" 2>"$stderr_file"
  EXIT_CODE=$?
  set -e
  STDOUT_TEXT="$(cat "$stdout_file")"
  STDERR_TEXT="$(cat "$stderr_file")"
  rm -f "$stdout_file" "$stderr_file"
}
RANDOM_ID=0

new_sandbox_home() {
  d="${WORK}/home-${RANDOM_ID}"
  RANDOM_ID=$((RANDOM_ID + 1))
  mkdir -p "$d"
  printf '%s' "$d"
}

new_sandbox_install_dir() {
  d="${WORK}/install-${RANDOM_ID}"
  RANDOM_ID=$((RANDOM_ID + 1))
  printf '%s' "$d"
}

build_restricted_path() {
  # A PATH containing every tool install.sh needs EXCEPT sha256sum/shasum,
  # for the fail-closed missing-checksum-tool case (INST-10).
  rp="${WORK}/restricted-bin-${RANDOM_ID}"
  RANDOM_ID=$((RANDOM_ID + 1))
  mkdir -p "$rp"
  for tool in sh curl mktemp chmod mv cp grep awk basename cat rm mkdir dirname uname printf; do
    p="$(command -v "$tool" 2>/dev/null || true)"
    [ -n "$p" ] && ln -sf "$p" "${rp}/${tool}"
  done
  printf '%s' "$rp"
}

# =========================================================================

echo "=== INST-07: latest resolves to the newest fixture (no LLM_WIKIS_VERSION) ==="
home1="$(new_sandbox_home)"; dir1="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home1" LLM_WIKIS_TEST_INSTALL_DIR="$dir1" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$home1"
[ "$EXIT_CODE" = "0" ] && pass "latest install exits 0" || fail "latest install exits 0 (stderr: $STDERR_TEXT)"
[ -x "${dir1}/llm-wikis" ] && pass "latest install places an executable at ${dir1}/llm-wikis" || fail "latest install places the binary"
case "$STDOUT_TEXT" in
  *"llm-wikis 0.1.0"*) pass "latest install's final --version prints the latest fixture's version" ;;
  *) fail "latest install's final --version output ($STDOUT_TEXT)" ;;
esac

echo "=== INST-07: pinned LLM_WIKIS_VERSION resolves to that exact tag ==="
home2="$(new_sandbox_home)"; dir2="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home2" LLM_WIKIS_TEST_INSTALL_DIR="$dir2" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$home2" \
  LLM_WIKIS_VERSION=v2.0.0
[ "$EXIT_CODE" = "0" ] && pass "pinned v2.0.0 install exits 0" || fail "pinned v2.0.0 install exits 0 (stderr: $STDERR_TEXT)"
case "$STDOUT_TEXT" in
  *"0.2.0-fixture"*) pass "pinned v2.0.0 install's final --version prints the v2.0.0 fixture's distinct version, not v1.0.0/latest" ;;
  *) fail "pinned v2.0.0 install's final --version output ($STDOUT_TEXT)" ;;
esac

echo "=== INST-08: unsupported architecture errors explicitly (Intel macOS) ==="
home3="$(new_sandbox_home)"; dir3="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home3" LLM_WIKIS_TEST_INSTALL_DIR="$dir3" \
  LLM_WIKIS_TEST_OS=Darwin LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/zsh HOME="$home3"
[ "$EXIT_CODE" != "0" ] && pass "Intel macOS install exits non-zero" || fail "Intel macOS install exits non-zero"
case "$STDERR_TEXT" in
  *"Intel macOS is not supported"*) pass "Intel macOS install prints the explicit Intel-macOS error" ;;
  *) fail "Intel macOS error text ($STDERR_TEXT)" ;;
esac
[ ! -e "${dir3}/llm-wikis" ] && pass "Intel macOS install places no binary" || fail "Intel macOS install placed a binary"

echo "=== INST-08: unsupported architecture errors explicitly (unsupported Linux arch) ==="
home4="$(new_sandbox_home)"; dir4="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home4" LLM_WIKIS_TEST_INSTALL_DIR="$dir4" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=armv7l SHELL=/bin/bash HOME="$home4"
[ "$EXIT_CODE" != "0" ] && pass "unsupported Linux arch install exits non-zero" || fail "unsupported Linux arch install exits non-zero"
case "$STDERR_TEXT" in
  *"unsupported Linux architecture"*) pass "unsupported Linux arch install prints an explicit error" ;;
  *) fail "unsupported Linux arch error text ($STDERR_TEXT)" ;;
esac
[ ! -e "${dir4}/llm-wikis" ] && pass "unsupported Linux arch install places no binary" || fail "unsupported Linux arch install placed a binary"

echo "=== INST-09/INST-15: temp dir is used and cleaned on both success and failure ==="
tmp_success="${WORK}/tmpdir-success"; mkdir -p "$tmp_success"
home5="$(new_sandbox_home)"; dir5="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home5" LLM_WIKIS_TEST_INSTALL_DIR="$dir5" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$home5" \
  TMPDIR="$tmp_success"
[ -z "$(ls -A "$tmp_success" 2>/dev/null)" ] && pass "TMPDIR sandbox is empty after a successful install (temp dir cleaned)" || fail "TMPDIR sandbox left entries after success: $(ls -A "$tmp_success")"

tmp_failure="${WORK}/tmpdir-failure"; mkdir -p "$tmp_failure"
home6="$(new_sandbox_home)"; dir6="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home6" LLM_WIKIS_TEST_INSTALL_DIR="$dir6" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$home6" \
  TMPDIR="$tmp_failure" LLM_WIKIS_VERSION=bad-checksum
[ -z "$(ls -A "$tmp_failure" 2>/dev/null)" ] && pass "TMPDIR sandbox is empty after a failed (checksum-mismatch) install (temp dir cleaned)" || fail "TMPDIR sandbox left entries after failure: $(ls -A "$tmp_failure")"

echo "=== INST-10: fails closed when neither sha256sum nor shasum is available ==="
restricted="$(build_restricted_path)"
home7="$(new_sandbox_home)"; dir7="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home7" LLM_WIKIS_TEST_INSTALL_DIR="$dir7" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$home7" \
  PATH="$restricted"
[ "$EXIT_CODE" != "0" ] && pass "missing-checksum-tool install exits non-zero" || fail "missing-checksum-tool install exits non-zero"
case "$STDERR_TEXT" in
  *"neither sha256sum nor shasum"*) pass "missing-checksum-tool install prints the explicit fail-closed error" ;;
  *) fail "missing-checksum-tool error text ($STDERR_TEXT)" ;;
esac
[ ! -e "${dir7}/llm-wikis" ] && pass "missing-checksum-tool install places no binary" || fail "missing-checksum-tool install placed a binary"

echo "=== INST-11: smoke-test failure (--version fails) aborts before install ==="
home8="$(new_sandbox_home)"; dir8="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home8" LLM_WIKIS_TEST_INSTALL_DIR="$dir8" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$home8" \
  LLM_WIKIS_VERSION=smoke-fail
[ "$EXIT_CODE" != "0" ] && pass "smoke-failure install exits non-zero" || fail "smoke-failure install exits non-zero"
case "$STDERR_TEXT" in
  *"smoke test"*) pass "smoke-failure install reports the --version smoke-test failure" ;;
  *) fail "smoke-failure error text ($STDERR_TEXT)" ;;
esac
[ ! -e "${dir8}/llm-wikis" ] && pass "smoke-failure install places no binary" || fail "smoke-failure install placed a binary"

echo "=== INST-12: successful install lands at exactly <install-dir>/llm-wikis (sandboxed ~/.local/bin) ==="
home9="$(new_sandbox_home)"; dir9="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$home9" LLM_WIKIS_TEST_INSTALL_DIR="$dir9" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$home9"
[ "$EXIT_CODE" = "0" ] && [ -x "${dir9}/llm-wikis" ] && pass "binary present at exactly <install-dir>/llm-wikis" || fail "binary not present at <install-dir>/llm-wikis"

echo "=== INST-13: idempotently adds the install dir to ~/.zprofile (macOS zsh), once across two runs ==="
homeZ="$(new_sandbox_home)"; dirZ="$(new_sandbox_install_dir)"
envZ() {
  run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
    LLM_WIKIS_TEST_HOME="$homeZ" LLM_WIKIS_TEST_INSTALL_DIR="$dirZ" \
    LLM_WIKIS_TEST_OS=Darwin LLM_WIKIS_TEST_ARCH=arm64 SHELL=/bin/zsh HOME="$homeZ"
}
envZ
[ "$EXIT_CODE" = "0" ] && pass "zsh run 1 exits 0" || fail "zsh run 1 exits 0 (stderr: $STDERR_TEXT)"
zprofile="${homeZ}/.zprofile"
count1=$(grep -Fc "$dirZ" "$zprofile" 2>/dev/null || echo 0)
[ "$count1" = "1" ] && pass "~/.zprofile contains the install dir exactly once after run 1" || fail "~/.zprofile occurrence count after run 1: $count1"
envZ
[ "$EXIT_CODE" = "0" ] && pass "zsh run 2 exits 0" || fail "zsh run 2 exits 0 (stderr: $STDERR_TEXT)"
count2=$(grep -Fc "$dirZ" "$zprofile" 2>/dev/null || echo 0)
[ "$count2" = "1" ] && pass "~/.zprofile still contains the install dir exactly once after run 2, no duplicate added" || fail "~/.zprofile occurrence count after run 2: $count2"

echo "=== INST-13: idempotently adds the install dir to ~/.profile (Linux/bash), once across two runs ==="
homeB="$(new_sandbox_home)"; dirB="$(new_sandbox_install_dir)"
envB() {
  run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
    LLM_WIKIS_TEST_HOME="$homeB" LLM_WIKIS_TEST_INSTALL_DIR="$dirB" \
    LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$homeB"
}
envB
[ "$EXIT_CODE" = "0" ] && pass "bash run 1 exits 0" || fail "bash run 1 exits 0 (stderr: $STDERR_TEXT)"
profileB="${homeB}/.profile"
count3=$(grep -Fc "$dirB" "$profileB" 2>/dev/null || echo 0)
[ "$count3" = "1" ] && pass "~/.profile contains the install dir exactly once after run 1" || fail "~/.profile occurrence count after run 1: $count3"
envB
[ "$EXIT_CODE" = "0" ] && pass "bash run 2 exits 0" || fail "bash run 2 exits 0 (stderr: $STDERR_TEXT)"
count4=$(grep -Fc "$dirB" "$profileB" 2>/dev/null || echo 0)
[ "$count4" = "1" ] && pass "~/.profile still contains the install dir exactly once after run 2, no duplicate added" || fail "~/.profile occurrence count after run 2: $count4"

echo "=== INST-14: unsupported shell prints instructions, modifies no profile ==="
homeU="$(new_sandbox_home)"; dirU="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$homeU" LLM_WIKIS_TEST_INSTALL_DIR="$dirU" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/usr/bin/fish HOME="$homeU"
[ "$EXIT_CODE" = "0" ] && pass "unsupported-shell install still exits 0 (install itself still succeeds)" || fail "unsupported-shell install exit code ($EXIT_CODE, stderr: $STDERR_TEXT)"
case "$STDOUT_TEXT" in
  *"add this to your PATH manually"*) pass "unsupported-shell install prints manual PATH instructions" ;;
  *) fail "unsupported-shell install stdout ($STDOUT_TEXT)" ;;
esac
if [ ! -e "${homeU}/.profile" ] && [ ! -e "${homeU}/.zprofile" ]; then
  pass "unsupported-shell install creates neither ~/.profile nor ~/.zprofile"
else
  fail "unsupported-shell install left a profile file: $(ls -A "$homeU" 2>/dev/null)"
fi

echo "=== INST-16: reinstall upgrades, downgrades, and repairs the binary ==="
homeR="$(new_sandbox_home)"; dirR="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$homeR" LLM_WIKIS_TEST_INSTALL_DIR="$dirR" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$homeR" \
  LLM_WIKIS_VERSION=v1.0.0
case "$STDOUT_TEXT" in
  *"llm-wikis 0.1.0"*) pass "pinned v1.0.0 install reports the v1.0.0 fixture's version" ;;
  *) fail "pinned v1.0.0 install stdout ($STDOUT_TEXT)" ;;
esac
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$homeR" LLM_WIKIS_TEST_INSTALL_DIR="$dirR" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$homeR" \
  LLM_WIKIS_VERSION=v2.0.0
case "$STDOUT_TEXT" in
  *"0.2.0-fixture"*) pass "re-running pinned to v2.0.0 upgrades in place to the v2.0.0 fixture's version" ;;
  *) fail "re-running pinned to v2.0.0 stdout ($STDOUT_TEXT)" ;;
esac
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$BASE_URL" \
  LLM_WIKIS_TEST_HOME="$homeR" LLM_WIKIS_TEST_INSTALL_DIR="$dirR" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$homeR" \
  LLM_WIKIS_VERSION=v1.0.0
case "$STDOUT_TEXT" in
  *"llm-wikis 0.1.0"*) pass "re-running pinned back to v1.0.0 downgrades in place to the v1.0.0 fixture's version again" ;;
  *) fail "re-running pinned back to v1.0.0 stdout ($STDOUT_TEXT)" ;;
esac

echo "=== defect-2b: unpinned 'latest' install fails with a clear, actionable message when no stable release exists ==="
# Separate, empty fixture root -- no 'latest/download' directory at all --
# so the download 404s exactly like a real repository with only
# pre-releases published (GitHub's releases/latest/download/... deliberately
# skips pre-releases).
NO_LATEST_ROOT="${WORK}/no-latest-fixture"
mkdir -p "$NO_LATEST_ROOT"
NO_LATEST_BASE_URL="file://${NO_LATEST_ROOT}"
homeNL="$(new_sandbox_home)"; dirNL="$(new_sandbox_install_dir)"
run_installer LLM_WIKIS_INSTALLER_TEST=1 LLM_WIKIS_TEST_BASE_URL="$NO_LATEST_BASE_URL" \
  LLM_WIKIS_TEST_HOME="$homeNL" LLM_WIKIS_TEST_INSTALL_DIR="$dirNL" \
  LLM_WIKIS_TEST_OS=Linux LLM_WIKIS_TEST_ARCH=x86_64 SHELL=/bin/bash HOME="$homeNL"
[ "$EXIT_CODE" != "0" ] && pass "unpinned install against a repository with no stable release exits non-zero" || fail "unpinned install against a repository with no stable release exits non-zero"
case "$STDERR_TEXT" in
  *"no stable release published yet"*) pass "unpinned install against a repository with no stable release prints the explicit actionable error, not a bare download failure" ;;
  *) fail "no-stable-release error text ($STDERR_TEXT)" ;;
esac
[ ! -e "${dirNL}/llm-wikis" ] && pass "unpinned install against a repository with no stable release places no binary" || fail "unpinned install against a repository with no stable release placed a binary"

echo ""
echo "=== Summary: ${PASS_COUNT} passed, ${FAIL_COUNT} failed ==="
if [ -n "$SKIP_NOTES" ]; then
  echo "Skipped:${SKIP_NOTES}"
fi

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 1
fi
exit 0
