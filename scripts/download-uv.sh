#!/usr/bin/env bash
# Download uv binary for macOS / Linux
# Usage: bash scripts/download-uv.sh [version]

set -euo pipefail

UV_VERSION="${1:-0.6.14}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESOURCES_DIR="$SCRIPT_DIR/../src-tauri/resources"

mkdir -p "$RESOURCES_DIR"
DEST="$RESOURCES_DIR/uv"

if [[ -f "$DEST" ]]; then
    echo "uv already exists at $DEST"
    "$DEST" --version
    exit 0
fi

# Detect platform + arch
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)
        case "$ARCH" in
            arm64)  TRIPLE="aarch64-apple-darwin" ;;
            x86_64) TRIPLE="x86_64-apple-darwin" ;;
            *) echo "Unsupported arch: $ARCH"; exit 1 ;;
        esac
        ;;
    Linux)
        case "$ARCH" in
            x86_64)  TRIPLE="x86_64-unknown-linux-gnu" ;;
            aarch64) TRIPLE="aarch64-unknown-linux-gnu" ;;
            *) echo "Unsupported arch: $ARCH"; exit 1 ;;
        esac
        ;;
    *)
        echo "Unsupported OS: $OS"; exit 1 ;;
esac

URL="https://github.com/astral-sh/uv/releases/download/$UV_VERSION/uv-$TRIPLE.tar.gz"
TMP_DIR="$(mktemp -d)"

echo "Downloading uv $UV_VERSION for $TRIPLE ..."
curl -fsSL "$URL" | tar -xz -C "$TMP_DIR"

# Archive extracts to uv-$TRIPLE/uv
cp "$TMP_DIR/uv-$TRIPLE/uv" "$DEST"
chmod +x "$DEST"
rm -rf "$TMP_DIR"

echo "uv installed to $DEST"
"$DEST" --version
