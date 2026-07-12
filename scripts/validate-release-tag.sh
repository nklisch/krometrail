#!/usr/bin/env bash
# Validate that a release tag names the exact version in the checked-out root Cargo package.
# Usage: bash scripts/validate-release-tag.sh v1.2.3 [Cargo.toml]

set -euo pipefail

TAG="${1:-}"
MANIFEST="${2:-Cargo.toml}"

if [[ -z "$TAG" ]]; then
	echo "release tag validation failure: a tag is required" >&2
	exit 1
fi

if [[ ! "$TAG" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	echo "release tag validation failure: expected strict v<major>.<minor>.<patch>, got '$TAG'" >&2
	exit 1
fi

if [[ ! -f "$MANIFEST" ]]; then
	echo "release tag validation failure: manifest not found: $MANIFEST" >&2
	exit 1
fi

version_lines="$(awk '
	/^\[package\][[:space:]]*$/ { in_package = 1; next }
	/^\[/ { in_package = 0 }
	in_package && /^[[:space:]]*version[[:space:]]*=[[:space:]]*"[^"#]+"/ { print; count++ }
	END { if (count != 1) exit 2 }
' "$MANIFEST")" || {
	echo "release tag validation failure: expected exactly one root package version in $MANIFEST" >&2
	exit 1
}

cargo_version="$(printf '%s\n' "$version_lines" | sed -E 's/^[[:space:]]*version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
if [[ ! "$cargo_version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	echo "release tag validation failure: root Cargo version is not strict semver: '$cargo_version'" >&2
	exit 1
fi

expected="v$cargo_version"
if [[ "$TAG" != "$expected" ]]; then
	echo "release tag validation failure: tag '$TAG' does not match root Cargo version '$expected'" >&2
	exit 1
fi

echo "release tag validated: $TAG"
