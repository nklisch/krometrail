#!/bin/sh
# Install krometrail CLI
# Usage: curl -fsSL https://krometrail.dev/install.sh | sh

set -e

REPO="nklisch/krometrail"
BINARY_NAME="krometrail"

# Detect OS
OS="$(uname -s)"
case "$OS" in
	Linux)  PLATFORM="linux" ;;
	Darwin) PLATFORM="darwin" ;;
	*)
		echo "Unsupported OS: $OS"
		exit 1
		;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
	x86_64|amd64)   ARCH_SUFFIX="x64" ;;
	aarch64|arm64)  ARCH_SUFFIX="arm64" ;;
	*)
		echo "Unsupported architecture: $ARCH"
		exit 1
		;;
esac

ASSET_NAME="${BINARY_NAME}-${PLATFORM}-${ARCH_SUFFIX}"

# Fetch latest release version from GitHub API
RELEASE_URL="https://api.github.com/repos/${REPO}/releases/latest"

echo "Fetching latest release..."

if command -v curl > /dev/null 2>&1; then
	RELEASE_JSON="$(curl -fsSL "$RELEASE_URL")"
elif command -v wget > /dev/null 2>&1; then
	RELEASE_JSON="$(wget -qO- "$RELEASE_URL")"
else
	echo "Error: curl or wget is required to install krometrail"
	exit 1
fi

# Extract tag_name from JSON (POSIX-compatible, no jq dependency)
VERSION="$(printf '%s' "$RELEASE_JSON" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"

if [ -z "$VERSION" ]; then
	echo "Error: could not determine latest release version"
	exit 1
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"

# Determine install directory
INSTALL_DIR="${KROMETRAIL_INSTALL:-$HOME/.local/bin}"
INSTALL_PATH="${INSTALL_DIR}/${BINARY_NAME}"

echo "Installing krometrail ${VERSION} (${PLATFORM}-${ARCH_SUFFIX}) to ${INSTALL_PATH}..."

mkdir -p "$INSTALL_DIR"

# Download binary
if command -v curl > /dev/null 2>&1; then
	curl -fsSL --output "$INSTALL_PATH" "$DOWNLOAD_URL"
elif command -v wget > /dev/null 2>&1; then
	wget -qO "$INSTALL_PATH" "$DOWNLOAD_URL"
fi

chmod +x "$INSTALL_PATH"

# Remove macOS quarantine attribute
if [ "$PLATFORM" = "darwin" ]; then
	xattr -d com.apple.quarantine "$INSTALL_PATH" 2>/dev/null || true
fi

echo "Installed: ${INSTALL_PATH}"

# Warn if install dir is not in PATH
case ":$PATH:" in
	*":${INSTALL_DIR}:"*) ;;
	*)
		echo ""
		echo "Warning: ${INSTALL_DIR} is not in your PATH."
		echo "Add the following to your shell profile to use krometrail:"
		echo ""
		echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
		echo ""
		;;
esac

echo "krometrail ${VERSION} installed successfully."
