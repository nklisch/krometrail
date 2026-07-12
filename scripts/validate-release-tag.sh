#!/usr/bin/env bash
# Validate an exact release tag and print the commit it names.
# Usage: bash scripts/validate-release-tag.sh v1.2.3 [Cargo.toml]
#
# The caller must fetch tags before invoking this script. The manifest is read
# from the resolved commit, not from the caller's current branch, so a branch
# with a release-shaped name can never supply release metadata accidentally.

set -euo pipefail

TAG="${1:-}"
MANIFEST="${2:-Cargo.toml}"

fail() {
	echo "release tag validation failure: $*" >&2
	exit 1
}

if [[ -z "$TAG" ]]; then
	fail "a tag is required"
fi

if [[ ! "$TAG" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	fail "expected strict v<major>.<minor>.<patch>, got '$TAG'"
fi

if ! git rev-parse --git-dir >/dev/null 2>&1; then
	fail "a Git repository is required to resolve refs/tags/$TAG"
fi

TAG_REF="refs/tags/$TAG"
if ! git show-ref --verify --quiet "$TAG_REF"; then
	fail "exact tag ref '$TAG_REF' does not exist"
fi

TAG_SHA="$(git rev-parse --verify "${TAG_REF}^{commit}")" || fail "tag '$TAG' does not resolve to a commit"
if [[ ! "$TAG_SHA" =~ ^[0-9a-f]{40}$ ]]; then
	fail "tag '$TAG' resolved to an invalid commit id: '$TAG_SHA'"
fi

manifest_text="$(git show "$TAG_SHA:$MANIFEST")" || fail "manifest not found at '$MANIFEST' in tag '$TAG'"
version_lines="$(printf '%s\n' "$manifest_text" | awk '
	/^\[package\][[:space:]]*$/ { in_package = 1; next }
	/^\[/ { in_package = 0 }
	in_package && /^[[:space:]]*version[[:space:]]*=[[:space:]]*"[^"]+"/ { print; count++ }
	END { if (count != 1) exit 2 }
')" || fail "expected exactly one root package version in $MANIFEST at tag '$TAG'"

cargo_version="$(printf '%s\n' "$version_lines" | sed -E 's/^[[:space:]]*version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/')"
if [[ ! "$cargo_version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	fail "root Cargo version is not strict semver: '$cargo_version'"
fi

expected="v$cargo_version"
if [[ "$TAG" != "$expected" ]]; then
	fail "tag '$TAG' does not match root Cargo version '$expected'"
fi

echo "release tag validated: $TAG -> $TAG_SHA" >&2
printf '%s\n' "$TAG_SHA"
