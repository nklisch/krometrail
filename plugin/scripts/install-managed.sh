#!/bin/sh
# Install one exact Krometrail release for the native plugin launcher.
# All diagnostics go to stderr; stdout remains available to the MCP child.
set -eu
umask 077

REPO="nklisch/krometrail"
MAX_CHECKSUM_BYTES=1048576
MAX_ARTIFACT_BYTES=67108864
MAX_REDIRECTS=4

VERSION="${1:-}"
MANAGED_ROOT="${2:-}"

fail() {
  printf 'krometrail plugin install error: %s\n' "$1" >&2
  exit 1
}

printf '%s\n' "$VERSION" | awk '
  length($0) <= 64 && $0 ~ /^[0-9]+[.][0-9]+[.][0-9]+$/ { valid=1 }
  END { exit(valid ? 0 : 1) }
' || fail "invalid managed release version"

case "$MANAGED_ROOT" in
  /*) ;;
  *) fail "managed data path must be absolute" ;;
esac
case "$MANAGED_ROOT" in
  *..*|*//*|*[[:cntrl:]]*) fail "managed data path contains an unsafe component" ;;
esac

for command in awk cat chmod curl dirname id mkdir mktemp mv sed stat tr uname wc; do
  command -v "$command" >/dev/null 2>&1 || fail "$command is required"
done

path_owner() {
  stat -c '%u' "$1" 2>/dev/null || stat -f '%u' "$1" 2>/dev/null
}

validate_existing_path() {
  cursor="$1"
  while [ "$cursor" != "/" ] && [ -n "$cursor" ]; do
    [ ! -L "$cursor" ] || fail "managed data path contains a symlink: $cursor"
    if [ -e "$cursor" ]; then
      [ -d "$cursor" ] || fail "managed data path component is not a directory: $cursor"
    fi
    cursor=$(dirname "$cursor")
  done
}

validate_existing_path "$MANAGED_ROOT"
mkdir -p "$MANAGED_ROOT" || fail "could not create managed data directory"
validate_existing_path "$MANAGED_ROOT"
[ -d "$MANAGED_ROOT" ] && [ -w "$MANAGED_ROOT" ] || fail "managed data directory is not writable"
owner=$(path_owner "$MANAGED_ROOT") || fail "could not verify managed data directory owner"
[ "$owner" = "$(id -u)" ] || fail "managed data directory must be owned by the current user"
chmod 700 "$MANAGED_ROOT" || fail "could not make managed data directory private"

VERSIONS_DIR="$MANAGED_ROOT/versions"
VERSION_DIR="$VERSIONS_DIR/$VERSION"
mkdir -p "$VERSION_DIR" || fail "could not create managed release directory"
for directory in "$VERSIONS_DIR" "$VERSION_DIR"; do
  [ ! -L "$directory" ] || fail "managed release directory must not be a symlink"
  owner=$(path_owner "$directory") || fail "could not verify managed release directory owner"
  [ "$owner" = "$(id -u)" ] || fail "managed release directory must be owned by the current user"
  chmod 700 "$directory" || fail "could not make managed release directory private"
done

DESTINATION="$VERSION_DIR/krometrail"
if [ -e "$DESTINATION" ] || [ -L "$DESTINATION" ]; then
  [ -f "$DESTINATION" ] && [ ! -L "$DESTINATION" ] || fail "managed binary destination is not a regular file"
  owner=$(path_owner "$DESTINATION") || fail "could not verify managed binary owner"
  [ "$owner" = "$(id -u)" ] || fail "managed binary must be owned by the current user"
fi

OS=$(uname -s)
ARCH=$(uname -m)
case "$OS" in
  Linux) platform=linux ;;
  Darwin) platform=darwin ;;
  *) fail "automatic plugin bootstrap is unsupported on $OS" ;;
esac
case "$ARCH" in
  x86_64|amd64) architecture=x64 ;;
  aarch64|arm64) architecture=arm64 ;;
  *) fail "automatic plugin bootstrap is unsupported on architecture $ARCH" ;;
esac
ASSET="krometrail-${platform}-${architecture}"
TAG="v$VERSION"
BASE_URL="https://github.com/$REPO/releases/download/$TAG"

validate_url() {
  case "$1" in
    https://github.com/*|https://objects.githubusercontent.com/*|https://release-assets.githubusercontent.com/*) ;;
    *) fail "release download redirected to an untrusted host" ;;
  esac
}

download() {
  url="$1"
  output="$2"
  max_bytes="$3"
  current_url="$url"
  redirects=0

  while [ "$redirects" -le "$MAX_REDIRECTS" ]; do
    validate_url "$current_url"
    metadata="${output}.http"
    rm -f "$metadata"
    if ! curl --silent --show-error --max-redirs 0 --proto '=https' --proto-redir '=https' \
      --connect-timeout 10 --max-time 90 --max-filesize "$max_bytes" \
      --user-agent 'krometrail-plugin-installer/1' --output "$output" \
      --write-out '%{http_code}\n%{redirect_url}\n' "$current_url" >"$metadata"; then
      rm -f "$metadata"
      fail "release download failed"
    fi

    status=$(sed -n '1p' "$metadata")
    redirect=$(sed -n '2p' "$metadata")
    rm -f "$metadata"
    case "$status" in
      2[0-9][0-9]) break ;;
      3[0-9][0-9])
        [ -n "$redirect" ] || fail "release redirect omitted its target"
        validate_url "$redirect"
        current_url="$redirect"
        redirects=$((redirects + 1))
        ;;
      *) fail "release download returned HTTP $status" ;;
    esac
  done

  [ "$redirects" -le "$MAX_REDIRECTS" ] || fail "release redirect chain exceeded its limit"
  [ -f "$output" ] || fail "release download did not create an artifact"
  bytes=$(wc -c <"$output" | tr -d '[:space:]')
  case "$bytes" in ''|*[!0-9]*) fail "download size could not be verified" ;; esac
  [ "$bytes" -le "$max_bytes" ] || fail "release download exceeds its size limit"
}

TMP_BINARY=$(mktemp "$VERSION_DIR/.krometrail.XXXXXX") || fail "could not create managed binary temporary file"
CHECKSUMS=$(mktemp "$VERSION_DIR/.checksums.XXXXXX") || {
  rm -f "$TMP_BINARY"
  fail "could not create checksum temporary file"
}
cleanup() {
  rm -f "$TMP_BINARY" "$TMP_BINARY.http" "$CHECKSUMS" "$CHECKSUMS.http"
}
trap cleanup EXIT HUP INT TERM

download "$BASE_URL/$ASSET" "$TMP_BINARY" "$MAX_ARTIFACT_BYTES"
download "$BASE_URL/checksums.txt" "$CHECKSUMS" "$MAX_CHECKSUM_BYTES"

expected=$(awk -v asset="$ASSET" '
  $2 == asset { if (found++) duplicate=1; value=$1 }
  END { if (duplicate || found != 1) exit 1; print value }
' "$CHECKSUMS") || fail "checksums do not contain exactly one $ASSET entry"
case "$expected" in ''|*[!0-9a-fA-F]*) fail "release checksum is malformed" ;; esac
[ "$(printf '%s' "$expected" | awk '{ print length }')" -eq 64 ] || fail "release checksum is malformed"

if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$TMP_BINARY" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
  actual=$(shasum -a 256 "$TMP_BINARY" | awk '{ print $1 }')
else
  fail "sha256sum or shasum is required"
fi
expected=$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')
actual=$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')
[ "$actual" = "$expected" ] || fail "checksum verification failed for $ASSET"

chmod 700 "$TMP_BINARY" || fail "could not make verified release executable"
identity=$("$TMP_BINARY" --version 2>/dev/null) || fail "verified release could not report its version"
[ "$identity" = "krometrail $VERSION" ] || fail "verified release identity does not match v$VERSION"

[ ! -L "$DESTINATION" ] || fail "managed binary destination became a symlink"
mv -f "$TMP_BINARY" "$DESTINATION" || fail "could not publish managed release"
TMP_BINARY=""
chmod 700 "$DESTINATION" || fail "could not make managed release private and executable"
owner=$(path_owner "$DESTINATION") || fail "could not verify published managed binary owner"
[ "$owner" = "$(id -u)" ] || fail "published managed binary must be owned by the current user"
[ "$("$DESTINATION" --version 2>/dev/null)" = "krometrail $VERSION" ] || fail "published managed release identity changed"

printf 'krometrail plugin: installed managed release v%s\n' "$VERSION" >&2
