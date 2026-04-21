#!/usr/bin/env sh
# install.sh — download and install the latest merges binary
# Usage: curl -fsSL https://raw.githubusercontent.com/alxekb/merges/main/install.sh | sh
set -eu

REPO="alxekb/merges"
INSTALL_DIR="${MERGES_INSTALL_DIR:-/usr/local/bin}"

# ── detect OS and architecture ────────────────────────────────────────────────

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  ARTIFACT="merges-linux-x86_64"  ;;
      aarch64|arm64) ARTIFACT="merges-linux-aarch64" ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      arm64)   ARTIFACT="merges-macos-aarch64" ;;
      x86_64)  ARTIFACT="merges-macos-x86_64"  ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    echo "Install from source: cargo install --git https://github.com/$REPO merges" >&2
    exit 1
    ;;
esac

# ── resolve latest release tag ────────────────────────────────────────────────

if command -v curl >/dev/null 2>&1; then
  FETCH="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
  FETCH="wget -qO-"
else
  echo "Neither curl nor wget found. Please install one and retry." >&2
  exit 1
fi

TAG=$($FETCH "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' \
  | head -1 \
  | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [ -z "$TAG" ]; then
  echo "Could not determine the latest release tag." >&2
  exit 1
fi

URL="https://github.com/$REPO/releases/download/$TAG/$ARTIFACT"

# ── download ──────────────────────────────────────────────────────────────────

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

echo "Downloading merges $TAG ($ARTIFACT)..."
$FETCH "$URL" > "$TMP"

# ── install ───────────────────────────────────────────────────────────────────

chmod +x "$TMP"

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP" "$INSTALL_DIR/merges"
else
  echo "Installing to $INSTALL_DIR (may prompt for sudo password)..."
  sudo mv "$TMP" "$INSTALL_DIR/merges"
fi

echo "Installed merges $TAG to $INSTALL_DIR/merges"
echo "Run: merges --help"
