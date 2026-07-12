#!/usr/bin/env bash
# Verify that a remote release tag still resolves to the immutable build SHA.
# Usage: bash scripts/verify-release-tag-identity.sh v1.2.3 <commit-sha> [remote]

set -euo pipefail

TAG="${1:-}"
EXPECTED_SHA="${2:-}"
REMOTE="${3:-origin}"

fail() {
	echo "release tag identity failure: $*" >&2
	exit 1
}

if [[ ! "$TAG" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
	fail "expected strict v<major>.<minor>.<patch>, got '$TAG'"
fi
if [[ ! "$EXPECTED_SHA" =~ ^[0-9a-f]{40}$ ]]; then
	fail "expected a 40-character commit SHA, got '$EXPECTED_SHA'"
fi

TAG_REF="refs/tags/$TAG"
actual_sha="$(git ls-remote "$REMOTE" "${TAG_REF}^{}" | awk -v ref="${TAG_REF}^{}" '$2 == ref { print $1; exit }')"
if [[ -z "$actual_sha" ]]; then
	actual_sha="$(git ls-remote "$REMOTE" "$TAG_REF" | awk -v ref="$TAG_REF" '$2 == ref { print $1; exit }')"
fi
if [[ "$actual_sha" != "$EXPECTED_SHA" ]]; then
	fail "remote tag '$TAG' resolves to '${actual_sha:-<missing>}', expected '$EXPECTED_SHA'"
fi

printf 'release tag identity verified: %s -> %s\n' "$TAG" "$EXPECTED_SHA"
