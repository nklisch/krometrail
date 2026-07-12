#!/usr/bin/env bash
# Install the Rust CLI binary to ~/.local/bin.
# Run after changing the runtime or after a release: bash scripts/dev-install.sh

set -euo pipefail

DEST="${KROMETRAIL_INSTALL_DIR:-$HOME/.local/bin}"
BINARY="target/release/krometrail"

echo "Building current Rust release binary..."
CARGO_TARGET_DIR=target cargo build --locked --release

mkdir -p "$DEST"
cp "$BINARY" "$DEST/krometrail"
chmod +x "$DEST/krometrail"

echo "Installed: $DEST/krometrail"
"$DEST/krometrail" --version
