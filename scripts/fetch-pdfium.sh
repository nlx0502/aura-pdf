#!/usr/bin/env bash
# Télécharge la bibliothèque PDFium pour macOS (binaires officiels de
# bblanchon/pdfium-binaries, même build que pdfium.dll pour Windows) dans
# src-tauri/resources/pdfium/.
#
#   scripts/fetch-pdfium.sh            # Mac Apple (arm64), par défaut
#   scripts/fetch-pdfium.sh x64        # Mac Intel
set -euo pipefail

BUILD="8086"
ARCH="${1:-arm64}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/src-tauri/resources/pdfium"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${BUILD}/pdfium-mac-${ARCH}.tgz" -o "$TMP/pdfium.tgz"
tar -xzf "$TMP/pdfium.tgz" -C "$TMP"
grep -q "BUILD=${BUILD}" "$TMP/VERSION"
cp "$TMP/lib/libpdfium.dylib" "$DEST/libpdfium.dylib"
echo "PDFium ${BUILD} (mac-${ARCH}) installé dans $DEST"
