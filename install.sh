#!/usr/bin/env bash
# install.sh — install the `vlt` binary from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/andresdigiovanni/vlt/main/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- --version v0.2.0
#   ./install.sh [--version vX.Y.Z]
#
# Installs to ~/.local/bin/vlt (no sudo). Verifies the download against the
# published SHA256SUMS. Re-running upgrades in place.
set -euo pipefail

REPO="andresdigiovanni/vlt"
BIN_DIR="$HOME/.local/bin"
VERSION=""

usage() {
  cat <<EOF
Usage: install.sh [--version vX.Y.Z]

Installs the vlt binary from GitHub Releases ($REPO) into ~/.local/bin.

Options:
  --version vX.Y.Z   Install a specific release instead of the latest one.
  --help             Show this help.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      [ $# -ge 2 ] || { echo "error: --version needs a value (e.g. v0.1.0)" >&2; exit 2; }
      VERSION="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

# --- downloader: pick once by availability (no silent fallback on failure) ---
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --retry 3 --retry-delay 1 "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO- "$1"; }
else
  echo "error: curl or wget is required" >&2
  exit 1
fi

# Wrap fetch so network failures die with a clean message and exit code 1
# instead of a raw curl/wget status under set -e.
try_fetch() { # <url> <dest-file> <what>
  if ! fetch "$1" > "$2"; then
    echo "error: could not download $3 from GitHub (network issue, or wrong --version?)" >&2
    exit 1
  fi
  [ -s "$2" ] || { echo "error: empty download for $3" >&2; exit 1; }
}

# --- platform detection -> release target triple ---
case "$(uname -s)" in
  Linux) OS="unknown-linux-gnu" ;;
  Darwin) OS="apple-darwin" ;;
  *)
    echo "error: unsupported OS '$(uname -s)' (vlt supports macOS and Linux)" >&2
    exit 1
    ;;
esac
case "$(uname -m)" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *)
    echo "error: unsupported architecture '$(uname -m)'" >&2
    exit 1
    ;;
esac
TARGET="$ARCH-$OS"

# --- checksum tool ---
if command -v sha256sum >/dev/null 2>&1; then
  SUM=(sha256sum)
elif command -v shasum >/dev/null 2>&1; then
  SUM=(shasum -a 256)
else
  echo "error: sha256sum or shasum is required to verify the download" >&2
  exit 1
fi

# --- resolve release tag ---
if [ -z "$VERSION" ]; then
  echo "==> Resolving latest release..."
  if ! VERSION="$(fetch "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)"; then
    echo "error: could not reach GitHub to resolve the latest release of $REPO (rate limit? pass --version v0.1.0)" >&2
    exit 1
  fi
  if [ -z "$VERSION" ]; then
    echo "error: could not determine the latest release of $REPO" >&2
    exit 1
  fi
fi
if ! [[ "$VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version must look like v0.1.0, got: $VERSION" >&2
  exit 2
fi

ASSET="vlt-$VERSION-$TARGET.tar.gz"
BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
echo "==> Installing $REPO $VERSION for $TARGET"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; rm -f "$BIN_DIR/.vlt.new" 2>/dev/null || true' EXIT

# --- download + verify ---
echo "==> Downloading $ASSET"
try_fetch "$BASE_URL/$ASSET" "$TMP/$ASSET" "$ASSET"
try_fetch "$BASE_URL/SHA256SUMS" "$TMP/SHA256SUMS" "SHA256SUMS"

echo "==> Verifying checksum"
if ! ( cd "$TMP" && grep "  $ASSET\$" SHA256SUMS | "${SUM[@]}" -c - ); then
  echo "error: $ASSET is not listed in SHA256SUMS or its checksum mismatches (wrong --version?)" >&2
  exit 1
fi

# --- install ---
echo "==> Extracting"
tar -xzf "$TMP/$ASSET" -C "$TMP"
[ -f "$TMP/vlt" ] || { echo "error: archive did not contain a vlt binary" >&2; exit 1; }

echo "==> Smoke-testing the binary"
if ! "$TMP/vlt" --version >/dev/null 2>&1; then
  echo "error: downloaded binary failed to run on this system" >&2
  echo "       (open an issue at https://github.com/$REPO/issues with your OS/arch)" >&2
  exit 1
fi

mkdir -p "$BIN_DIR"
chmod +x "$TMP/vlt"
mv "$TMP/vlt" "$BIN_DIR/.vlt.new"
mv "$BIN_DIR/.vlt.new" "$BIN_DIR/vlt"   # atomic swap, upgrades in place

echo "==> Installed: $("$BIN_DIR/vlt" --version 2>/dev/null || echo "vlt $VERSION") at $BIN_DIR/vlt"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    echo "NOTE: $BIN_DIR is not on your PATH. Add this to your shell profile:"
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac

cat <<EOF

Next step: set up vlt (vault, models, MCP server and Skill for your agent):

  vlt install

Docs: https://github.com/$REPO#readme
EOF
