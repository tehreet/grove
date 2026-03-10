#!/bin/sh
set -e

REPO="tehreet/grove"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64|amd64) ARCH="amd64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

case "$OS" in
  linux|darwin) ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

ASSET="grove-${OS}-${ARCH}"
URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

echo "Downloading grove for ${OS}/${ARCH}..."
curl -fsSL -o /tmp/grove "$URL"
chmod +x /tmp/grove

INSTALL_DIR="${GROVE_INSTALL_DIR:-/usr/local/bin}"

if [ -w "$INSTALL_DIR" ]; then
  mv /tmp/grove "$INSTALL_DIR/grove"
else
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  sudo mv /tmp/grove "$INSTALL_DIR/grove"
fi

echo "grove installed to $INSTALL_DIR/grove"
grove --version
