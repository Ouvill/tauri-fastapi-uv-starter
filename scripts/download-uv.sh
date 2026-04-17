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

ASSET_NAME="uv-$TRIPLE.tar.gz"
BASE_URL="https://github.com/astral-sh/uv/releases/download/$UV_VERSION"
URL="$BASE_URL/$ASSET_NAME"
CHECKSUM_URL="$BASE_URL/SHA256SUMS"
TMP_DIR="$(mktemp -d)"
TMP_ARCHIVE="$TMP_DIR/$ASSET_NAME"
TMP_CHECKSUMS="$TMP_DIR/SHA256SUMS"

echo "Downloading uv $UV_VERSION for $TRIPLE ..."
curl -fsSL "$URL" -o "$TMP_ARCHIVE"

echo "Downloading checksums ..."
curl -fsSL "$CHECKSUM_URL" -o "$TMP_CHECKSUMS"

EXPECTED_HASH="$(grep "  $ASSET_NAME$" "$TMP_CHECKSUMS" | awk '{print $1}')"
if [[ -z "$EXPECTED_HASH" ]]; then
    echo "Failed to find checksum entry for $ASSET_NAME"
    rm -rf "$TMP_DIR"
    exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL_HASH="$(sha256sum "$TMP_ARCHIVE" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
    ACTUAL_HASH="$(shasum -a 256 "$TMP_ARCHIVE" | awk '{print $1}')"
elif command -v openssl >/dev/null 2>&1; then
    ACTUAL_HASH="$(openssl dgst -sha256 "$TMP_ARCHIVE" | awk '{print $2}')"
else
    echo "No SHA256 tool found (sha256sum/shasum/openssl)"
    rm -rf "$TMP_DIR"
    exit 1
fi

if [[ "$EXPECTED_HASH" != "$ACTUAL_HASH" ]]; then
    echo "SHA256 mismatch for $ASSET_NAME"
    echo "expected=$EXPECTED_HASH"
    echo "actual=$ACTUAL_HASH"
    rm -rf "$TMP_DIR"
    exit 1
fi

echo "Checksum verified for $ASSET_NAME"
tar -xzf "$TMP_ARCHIVE" -C "$TMP_DIR"

# Archive extracts to uv-$TRIPLE/uv
cp "$TMP_DIR/uv-$TRIPLE/uv" "$DEST"
chmod +x "$DEST"
rm -rf "$TMP_DIR"

echo "uv installed to $DEST"
"$DEST" --version
