#!/bin/sh
# llm-wikis installer for Linux x86-64 and macOS Apple Silicon.
# Follows the apm-go release-download flow (see spec 0.2.3 §18.2):
#   https://raw.githubusercontent.com/gn00678465/apm-go/refs/heads/main/install.sh
#
# Production always downloads from the fixed GitHub repository below. The
# LLM_WIKIS_TEST_* overrides are inert unless LLM_WIKIS_INSTALLER_TEST=1 is
# also set, so a real end-user run can never be redirected to another origin
# or install target.
set -eu

REPO="gn00678465/llm-wikis"
BASE_URL="https://github.com/${REPO}/releases"
INSTALL_DIR="${HOME}/.local/bin"
PROFILE_HOME="${HOME}"

if [ "${LLM_WIKIS_INSTALLER_TEST:-}" = "1" ]; then
  [ -n "${LLM_WIKIS_TEST_BASE_URL:-}" ] && BASE_URL="$LLM_WIKIS_TEST_BASE_URL"
  [ -n "${LLM_WIKIS_TEST_INSTALL_DIR:-}" ] && INSTALL_DIR="$LLM_WIKIS_TEST_INSTALL_DIR"
  [ -n "${LLM_WIKIS_TEST_HOME:-}" ] && PROFILE_HOME="$LLM_WIKIS_TEST_HOME"
  OS="${LLM_WIKIS_TEST_OS:-$(uname -s)}"
  ARCH="${LLM_WIKIS_TEST_ARCH:-$(uname -m)}"
else
  OS="$(uname -s)"
  ARCH="$(uname -m)"
fi

err() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

info() {
  printf '%s\n' "$1"
}

command -v curl >/dev/null 2>&1 || err "curl is required"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64) ASSET="llm-wikis-linux-amd64" ;;
      *) err "unsupported Linux architecture: ${ARCH} (llm-wikis supports Linux x86-64 only)" ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      arm64) ASSET="llm-wikis-darwin-arm64" ;;
      x86_64) err "unsupported platform: Intel macOS is not supported (llm-wikis supports Apple Silicon only)" ;;
      *) err "unsupported macOS architecture: ${ARCH}" ;;
    esac
    ;;
  *)
    err "unsupported operating system: ${OS} (llm-wikis supports Linux x86-64 and macOS Apple Silicon only)"
    ;;
esac

TMP_DIR="$(mktemp -d)"
INSTALL_TMP=""
cleanup() {
  rm -rf "$TMP_DIR"
  [ -n "$INSTALL_TMP" ] && rm -f "$INSTALL_TMP"
  return 0
}
trap cleanup EXIT INT TERM

VERSION="${LLM_WIKIS_VERSION:-}"
if [ -n "$VERSION" ]; then
  DOWNLOAD_BASE="${BASE_URL}/download/${VERSION}"
else
  DOWNLOAD_BASE="${BASE_URL}/latest/download"
fi

ASSET_PATH="${TMP_DIR}/${ASSET}"
SUMS_PATH="${TMP_DIR}/SHA256SUMS"

info "Downloading ${ASSET}..."
curl -fsSL -o "$ASSET_PATH" "${DOWNLOAD_BASE}/${ASSET}" || err "download failed: ${ASSET}"
curl -fsSL -o "$SUMS_PATH" "${DOWNLOAD_BASE}/SHA256SUMS" || err "download failed: SHA256SUMS"

if command -v sha256sum >/dev/null 2>&1; then
  HASH_OF() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  HASH_OF() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  err "neither sha256sum nor shasum is available; refusing to install an unverified binary"
fi

EXPECTED="$(awk -v f="$ASSET" '$2==f {print $1}' "$SUMS_PATH")"
[ -n "$EXPECTED" ] || err "no checksum entry for ${ASSET} in SHA256SUMS"
ACTUAL="$(HASH_OF "$ASSET_PATH")"
[ "$EXPECTED" = "$ACTUAL" ] || err "checksum mismatch for ${ASSET}: expected ${EXPECTED}, got ${ACTUAL}"

chmod +x "$ASSET_PATH"
"$ASSET_PATH" --version >/dev/null 2>&1 || err "downloaded binary failed smoke test (--version)"

mkdir -p "$INSTALL_DIR"
INSTALL_TMP="${INSTALL_DIR}/.llm-wikis.tmp.$$"
cp "$ASSET_PATH" "$INSTALL_TMP"
chmod +x "$INSTALL_TMP"
mv "$INSTALL_TMP" "${INSTALL_DIR}/llm-wikis"
INSTALL_TMP=""

info "Installed llm-wikis to ${INSTALL_DIR}/llm-wikis"

SHELL_NAME="$(basename "${SHELL:-}")"
case "$SHELL_NAME" in
  zsh) PROFILE="${PROFILE_HOME}/.zprofile" ;;
  bash | sh | dash | ash) PROFILE="${PROFILE_HOME}/.profile" ;;
  *) PROFILE="" ;;
esac

if [ -z "$PROFILE" ]; then
  info "Could not detect a supported shell (\$SHELL=${SHELL:-unset}); add this to your PATH manually:"
  info "  export PATH=\"${INSTALL_DIR}:\$PATH\""
else
  LINE="export PATH=\"${INSTALL_DIR}:\$PATH\""
  if [ -f "$PROFILE" ] && grep -qF "$LINE" "$PROFILE"; then
    : # already present; idempotent no-op
  else
    printf '\n# added by the llm-wikis installer\n%s\n' "$LINE" >> "$PROFILE"
    info "Added ${INSTALL_DIR} to PATH in ${PROFILE}. Restart your shell or run: . ${PROFILE}"
  fi
fi

"${INSTALL_DIR}/llm-wikis" --version
