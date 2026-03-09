#!/usr/bin/env bash
# Install the agent-lens CLI binary to ~/.local/bin
# Run after every release: bash scripts/install.sh

set -euo pipefail

DEST="${AGENT_LENS_INSTALL_DIR:-$HOME/.local/bin}"
BINARY="dist/agent-lens"

if [ ! -f "$BINARY" ]; then
  echo "Building..."
  bun run build
fi

mkdir -p "$DEST"
cp "$BINARY" "$DEST/agent-lens"
chmod +x "$DEST/agent-lens"

echo "Installed: $DEST/agent-lens"
"$DEST/agent-lens" --version 2>/dev/null || true
