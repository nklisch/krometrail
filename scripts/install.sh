#!/bin/sh
# Krometrail installer
# Usage: curl -fsSL https://krometrail.dev/install.sh | sh
#        curl -fsSL https://krometrail.dev/install.sh | sh -s -- --version v0.2.0
#        KROMETRAIL_INSTALL_DIR=/usr/local/bin curl -fsSL https://krometrail.dev/install.sh | sh

set -eu

REPO="nklisch/krometrail"
BINARY_NAME="krometrail"
GITHUB="https://github.com/${REPO}"

# --- Color output ---

setup_colors() {
	if [ -t 1 ] && [ -t 2 ]; then
		BOLD='\033[1m'
		GREEN='\033[0;32m'
		YELLOW='\033[0;33m'
		RED='\033[0;31m'
		CYAN='\033[0;36m'
		RESET='\033[0m'
	else
		BOLD=''
		GREEN=''
		YELLOW=''
		RED=''
		CYAN=''
		RESET=''
	fi
}

info()  { printf "${BOLD}${CYAN}info${RESET}  %s\n" "$1"; }
ok()    { printf "${BOLD}${GREEN}  ok${RESET}  %s\n" "$1"; }
warn()  { printf "${BOLD}${YELLOW}warn${RESET}  %s\n" "$1" >&2; }
err()   { printf "${BOLD}${RED} err${RESET}  %s\n" "$1" >&2; }

# --- Parse arguments ---

VERSION=""
INSTALL_DIR="${KROMETRAIL_INSTALL_DIR:-}"
NO_MODIFY_PATH=0

usage() {
	cat <<EOF
Krometrail installer

Usage:
  curl -fsSL https://krometrail.dev/install.sh | sh
  curl -fsSL https://krometrail.dev/install.sh | sh -s -- [OPTIONS]

Options:
  --version VERSION        Install a specific version (e.g. v0.2.0)
  --install-dir DIR        Install to DIR (default: ~/.local/bin)
  --no-modify-path         Don't offer to modify shell PATH
  -h, --help               Show this help

Environment variables:
  KROMETRAIL_INSTALL_DIR   Same as --install-dir
EOF
	exit 0
}

while [ $# -gt 0 ]; do
	case "$1" in
		--version)
			VERSION="$2"
			shift 2
			;;
		--install-dir)
			INSTALL_DIR="$2"
			shift 2
			;;
		--no-modify-path)
			NO_MODIFY_PATH=1
			shift
			;;
		-h|--help)
			setup_colors
			usage
			;;
		*)
			setup_colors
			err "Unknown option: $1"
			echo ""
			usage
			;;
	esac
done

# --- Detect platform ---

detect_platform() {
	OS="$(uname -s)"
	case "$OS" in
		Linux)   PLATFORM="linux" ;;
		Darwin)  PLATFORM="darwin" ;;
		MINGW*|MSYS*|CYGWIN*)
			err "Windows is not supported by this installer."
			echo ""
			echo "Download the binary directly from:"
			echo "  ${GITHUB}/releases/latest/download/krometrail-windows-x64.exe"
			exit 1
			;;
		*)
			err "Unsupported OS: $OS"
			exit 1
			;;
	esac

	ARCH="$(uname -m)"
	case "$ARCH" in
		x86_64|amd64)  ARCH_SUFFIX="x64" ;;
		aarch64|arm64) ARCH_SUFFIX="arm64" ;;
		*)
			err "Unsupported architecture: $ARCH"
			exit 1
			;;
	esac
}

# --- HTTP client ---

has_cmd() { command -v "$1" > /dev/null 2>&1; }

download() {
	url="$1"
	dest="$2"
	if has_cmd curl; then
		curl -fsSL --output "$dest" "$url"
	elif has_cmd wget; then
		wget -qO "$dest" "$url"
	else
		err "curl or wget is required"
		exit 1
	fi
}

fetch() {
	url="$1"
	if has_cmd curl; then
		curl -fsSL "$url"
	elif has_cmd wget; then
		wget -qO- "$url"
	else
		err "curl or wget is required"
		exit 1
	fi
}

# --- Resolve version ---

resolve_version() {
	if [ -n "$VERSION" ]; then
		# Ensure version starts with v
		case "$VERSION" in
			v*) ;;
			*)  VERSION="v${VERSION}" ;;
		esac
		info "Using requested version: ${VERSION}"
		return
	fi

	info "Fetching latest release..."
	RELEASE_JSON="$(fetch "https://api.github.com/repos/${REPO}/releases/latest")"

	VERSION="$(printf '%s' "$RELEASE_JSON" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"

	if [ -z "$VERSION" ]; then
		err "Could not determine latest release version"
		echo ""
		echo "This may be a GitHub API rate limit. Try:"
		echo "  curl -fsSL https://krometrail.dev/install.sh | sh -s -- --version v0.2.0"
		exit 1
	fi
}

# --- Checksum verification ---

verify_checksum() {
	binary_path="$1"
	asset_name="$2"
	checksums_url="${GITHUB}/releases/download/${VERSION}/checksums.txt"

	info "Verifying checksum..."

	checksums_file="$(mktemp)"
	if ! download "$checksums_url" "$checksums_file" 2>/dev/null; then
		rm -f "$checksums_file"
		err "Checksums are not available for ${VERSION}; refusing an unverified install"
		exit 1
	fi

	expected="$(awk -v asset="$asset_name" '$2 == asset { print $1 }' "$checksums_file")"
	rm -f "$checksums_file"

	case "$expected" in
		""|*[!0123456789abcdefABCDEF]*)
			err "No valid checksum found for ${asset_name}"
			exit 1
			;;
	esac

	if [ "$(printf '%s' "$expected" | wc -c | tr -d ' ')" -ne 64 ]; then
		err "Invalid checksum length for ${asset_name}"
		exit 1
	fi

	if has_cmd sha256sum; then
		actual="$(sha256sum "$binary_path" | awk '{print $1}')"
	elif has_cmd shasum; then
		actual="$(shasum -a 256 "$binary_path" | awk '{print $1}')"
	else
		err "sha256sum or shasum is required to verify the download"
		exit 1
	fi

	if [ "$actual" != "$expected" ]; then
		err "Checksum mismatch!"
		echo "  Expected: ${expected}"
		echo "  Got:      ${actual}"
		rm -f "$binary_path"
		exit 1
	fi

	ok "Checksum verified"
}

# --- PATH management ---

add_to_path() {
	install_dir="$1"

	# Already in PATH
	case ":${PATH}:" in
		*":${install_dir}:"*) return ;;
	esac

	if [ "$NO_MODIFY_PATH" -eq 1 ]; then
		warn "${install_dir} is not in your PATH"
		echo ""
		echo "  Add manually:  export PATH=\"${install_dir}:\$PATH\""
		echo ""
		return
	fi

	# Detect shell profile
	shell_name="$(basename "${SHELL:-/bin/sh}")"
	profile=""
	line="export PATH=\"${install_dir}:\$PATH\""

	case "$shell_name" in
		zsh)
			profile="$HOME/.zshrc"
			;;
		bash)
			if [ -f "$HOME/.bashrc" ]; then
				profile="$HOME/.bashrc"
			elif [ -f "$HOME/.bash_profile" ]; then
				profile="$HOME/.bash_profile"
			else
				profile="$HOME/.bashrc"
			fi
			;;
		fish)
			profile="$HOME/.config/fish/config.fish"
			line="fish_add_path ${install_dir}"
			;;
		*)
			profile="$HOME/.profile"
			;;
	esac

	# Check if already present
	if [ -f "$profile" ] && grep -qF "$install_dir" "$profile" 2>/dev/null; then
		return
	fi

	# Non-interactive: just add it
	if [ ! -t 0 ]; then
		mkdir -p "$(dirname "$profile")"
		printf '\n# Krometrail\n%s\n' "$line" >> "$profile"
		ok "Added ${install_dir} to ${profile}"
		echo ""
		echo "  Restart your shell or run:  source ${profile}"
		echo ""
		return
	fi

	# Interactive: ask
	printf "\n${YELLOW}${install_dir}${RESET} is not in your PATH.\n"
	printf "Add it to ${CYAN}${profile}${RESET}? [Y/n] "
	read -r answer </dev/tty
	case "$answer" in
		n|N|no|No|NO)
			echo ""
			echo "  Add manually:  ${line}"
			echo ""
			;;
		*)
			mkdir -p "$(dirname "$profile")"
			printf '\n# Krometrail\n%s\n' "$line" >> "$profile"
			ok "Added ${install_dir} to ${profile}"
			echo ""
			echo "  Restart your shell or run:  source ${profile}"
			echo ""
			;;
	esac
}

# --- Main ---

main() {
	setup_colors

	echo ""
	printf "${BOLD}Krometrail Installer${RESET}\n"
	echo ""

	detect_platform

	resolve_version

	case "${PLATFORM}-${ARCH_SUFFIX}" in
		linux-x64)   ASSET_NAME="krometrail-linux-x64" ;;
		linux-arm64) ASSET_NAME="krometrail-linux-arm64" ;;
		darwin-x64)  ASSET_NAME="krometrail-darwin-x64" ;;
		darwin-arm64) ASSET_NAME="krometrail-darwin-arm64" ;;
		*)
			err "No release asset is available for ${PLATFORM}-${ARCH_SUFFIX}"
			exit 1
			;;
	esac
	DOWNLOAD_URL="${GITHUB}/releases/download/${VERSION}/${ASSET_NAME}"

	# Default install directory
	if [ -z "$INSTALL_DIR" ]; then
		INSTALL_DIR="$HOME/.local/bin"
	fi
	INSTALL_PATH="${INSTALL_DIR}/${BINARY_NAME}"

	info "Installing ${BINARY_NAME} ${VERSION} (${PLATFORM}-${ARCH_SUFFIX})"
	info "Location: ${INSTALL_PATH}"

	# Create install directory
	mkdir -p "$INSTALL_DIR"

	# Download to temp file first (atomic install)
	TMP_FILE="$(mktemp "${INSTALL_DIR}/${BINARY_NAME}.XXXXXX")"
	trap 'rm -f "$TMP_FILE"' EXIT

	info "Downloading ${DOWNLOAD_URL}..."
	if ! download "$DOWNLOAD_URL" "$TMP_FILE"; then
		err "Download failed"
		echo ""
		echo "  Check that version ${VERSION} exists:"
		echo "  ${GITHUB}/releases/tag/${VERSION}"
		exit 1
	fi

	verify_checksum "$TMP_FILE" "$ASSET_NAME"

	# Atomic move into place
	chmod +x "$TMP_FILE"
	mv -f "$TMP_FILE" "$INSTALL_PATH"
	trap - EXIT

	# Remove macOS quarantine attribute
	if [ "$PLATFORM" = "darwin" ]; then
		xattr -d com.apple.quarantine "$INSTALL_PATH" 2>/dev/null || true
	fi

	ok "Installed ${BINARY_NAME} ${VERSION} to ${INSTALL_PATH}"

	# Verify it runs
	if "$INSTALL_PATH" --version > /dev/null 2>&1; then
		installed_version="$("$INSTALL_PATH" --version 2>/dev/null || echo "${VERSION}")"
		ok "Verified: ${installed_version}"
	fi

	add_to_path "$INSTALL_DIR"

	echo "  Get started:  krometrail doctor"
	echo ""
}

main
