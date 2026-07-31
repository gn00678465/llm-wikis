#!/bin/sh
# Verifies a release asset directory's SHA256SUMS manifest (specification
# §18.1; plan Task 14 Step 7): exactly five entries -- the three platform
# binaries plus both installers, never the manifest itself -- no missing,
# duplicate, or unexpected entries, and every listed hash matches the actual
# file in the directory.
#
# Usage: sh tests/release/verify-assets.sh <asset-directory>
#
# Used by .github/workflows/release.yml right after SHA256SUMS is generated,
# and runnable standalone against any asset directory (a real release
# download, or a synthetic local fixture) with no network access.
set -eu

DIR="${1:?usage: verify-assets.sh <asset-directory>}"
SUMS="${DIR}/SHA256SUMS"

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

[ -d "$DIR" ] || fail "asset directory not found: $DIR"
[ -f "$SUMS" ] || fail "SHA256SUMS not found at $SUMS"

if command -v sha256sum >/dev/null 2>&1; then
  hash_of() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  hash_of() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  fail "neither sha256sum nor shasum is available"
fi

EXPECTED_NAMES="llm-wikis-windows-amd64.exe
llm-wikis-linux-amd64
llm-wikis-darwin-arm64
install.sh
install.ps1"

LINE_COUNT=$(grep -c . "$SUMS" || true)
[ "$LINE_COUNT" = "5" ] || fail "SHA256SUMS has $LINE_COUNT entries, expected exactly 5"

# Binary-mode ("*"-prefixed) entries would silently break install.sh's own
# `awk '$2==f'` exact-match lookup -- reject the manifest outright if found.
if grep -Eq '^[0-9a-fA-F]+ +\*' "$SUMS"; then
  fail "SHA256SUMS contains a binary-mode '*' prefix; install.sh's awk lookup would not match it"
fi

NAMES_IN_SUMS=$(awk '{print $2}' "$SUMS" | sed 's/^\*//')

DUP=$(printf '%s\n' "$NAMES_IN_SUMS" | sort | uniq -d)
[ -z "$DUP" ] || fail "duplicate filename(s) in SHA256SUMS: $DUP"

for name in $EXPECTED_NAMES; do
  count=$(printf '%s\n' "$NAMES_IN_SUMS" | grep -Fxc "$name" || true)
  [ "$count" = "1" ] || fail "expected exactly one SHA256SUMS entry for $name, found $count"
  [ -f "${DIR}/${name}" ] || fail "expected asset file missing from directory: $name"
done

for name in $NAMES_IN_SUMS; do
  case "$name" in
    llm-wikis-windows-amd64.exe | llm-wikis-linux-amd64 | llm-wikis-darwin-arm64 | install.sh | install.ps1) : ;;
    *) fail "unexpected entry in SHA256SUMS: $name" ;;
  esac
done

while read -r expected_hash file_name; do
  file_name="${file_name#\*}"
  actual_hash="$(hash_of "${DIR}/${file_name}")"
  [ "$expected_hash" = "$actual_hash" ] || fail "hash mismatch for $file_name: expected $expected_hash, got $actual_hash"
done < "$SUMS"

FILE_COUNT=$(find "$DIR" -maxdepth 1 -type f | wc -l | tr -d ' ')
[ "$FILE_COUNT" = "6" ] || fail "asset directory contains $FILE_COUNT files, expected exactly 6 (5 assets + SHA256SUMS)"

echo "PASS: SHA256SUMS in $DIR has exactly the 5 expected entries, all hashes match"
