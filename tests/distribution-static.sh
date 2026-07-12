#!/usr/bin/env bash
# Static contract tests for release assets, installation, and version ownership.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE="$ROOT/.github/workflows/release.yml"
CI="$ROOT/.github/workflows/ci.yml"
INSTALLER="$ROOT/scripts/install.sh"
DEV_INSTALLER="$ROOT/scripts/dev-install.sh"
BUMP="$ROOT/scripts/bump-version.ts"
PACKAGE="$ROOT/package.json"
CARGO="$ROOT/Cargo.toml"

fail() {
	echo "distribution contract failure: $*" >&2
	exit 1
}

require_text() {
	local file="$1"
	local text="$2"
	grep -Fq -- "$text" "$file" || fail "${file#"$ROOT"} is missing: $text"
}

assets=(
	krometrail-linux-x64
	krometrail-linux-arm64
	krometrail-darwin-x64
	krometrail-darwin-arm64
	krometrail-windows-x64.exe
)

for asset in "${assets[@]}"; do
	require_text "$RELEASE" "asset: $asset"
	require_text "$RELEASE" "dist/$asset"
done

require_text "$RELEASE" "sha256sum"
require_text "$RELEASE" "dist/checksums.txt"
require_text "$RELEASE" "actions/attest-build-provenance@v2"
require_text "$RELEASE" "actions/upload-artifact@v4"
require_text "$RELEASE" "actions/download-artifact@v4"

# The installer deliberately spells out the supported platform mapping so an
# asset rename cannot silently produce a broken download URL.
for mapping in \
	'linux-x64)' \
	'linux-arm64)' \
	'darwin-x64)' \
	'darwin-arm64)'; do
	require_text "$INSTALLER" "$mapping"
done
for asset in \
	krometrail-linux-x64 \
	krometrail-linux-arm64 \
	krometrail-darwin-x64 \
	krometrail-darwin-arm64 \
	krometrail-windows-x64.exe; do
	require_text "$INSTALLER" "$asset"
done
require_text "$INSTALLER" 'releases/download/${VERSION}/checksums.txt'
require_text "$INSTALLER" 'awk -v asset="$asset_name"' 
if grep -Fq -- 'skipping verification' "$INSTALLER"; then
	fail "installer must fail rather than skip checksum verification"
fi

require_text "$CI" 'cargo fmt --all --check'
require_text "$CI" 'cargo check --workspace --all-targets --locked'
require_text "$CI" 'cargo test --workspace --all-targets --locked'
require_text "$CI" 'cargo clippy --workspace --all-targets --locked -- -D warnings'
require_text "$CI" 'bash tests/distribution-static.sh'

require_text "$DEV_INSTALLER" 'target/release/krometrail'
if grep -Fq -- 'bun run build' "$DEV_INSTALLER"; then
	fail "developer installation must not invoke a Bun product build"
fi

# Cargo.toml is the only product-version file. The workspace version is Cargo
# metadata inherited by member crates and is kept in sync by the bump script.
package_versions="$(awk '
	/^\[package\][[:space:]]*$/ { in_package=1; next }
	/^\[/ { in_package=0 }
	in_package && /^[[:space:]]*version[[:space:]]*=/ { count++ }
	END { print count + 0 }
' "$CARGO")"
test "$package_versions" -eq 1 || fail "Cargo.toml must have exactly one root package version"
require_text "$CARGO" '[workspace.package]'

for field in '"version"[[:space:]]*:' '"bin"[[:space:]]*:' '"main"[[:space:]]*:' '"types"[[:space:]]*:'; do
	if grep -Eq -- "$field" "$PACKAGE"; then
		fail "package.json must not contain product field $field"
	fi
done
require_text "$PACKAGE" '"private": true'
if grep -Eiq -- 'npm[[:space:]]+publish|bun[[:space:]]+run[[:space:]]+(build|test)' "$PACKAGE"; then
	fail "package.json must not expose product build, test, or publish scripts"
fi
if grep -Eiq -- 'bun[[:space:]]+run[[:space:]]+build|npm[[:space:]]+publish' "$RELEASE"; then
	fail "release workflow must not build or publish the Bun package"
fi

# Exercise the version bump's write path in an isolated throwaway Cargo repo.
# --prepare runs the Rust gates but intentionally performs no commit, tag, or push.
tmp="$(mktemp -d)"
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
mkdir -p "$tmp/src"
cat > "$tmp/Cargo.toml" <<'EOF'
[package]
name = "version-bump-fixture"
version = "1.2.3"
edition = "2024"

[workspace.package]
version = "1.2.3"
EOF
printf 'fn main() {}\n' > "$tmp/src/main.rs"
cp "$BUMP" "$tmp/bump-version.ts"
git -C "$tmp" init -q
git -C "$tmp" config user.email test@example.invalid
git -C "$tmp" config user.name distribution-test
( cd "$tmp" && bun bump-version.ts minor --prepare )
test "$(grep -Ec '^version = "1\.3\.0"$' "$tmp/Cargo.toml")" -eq 2 || fail "prepare mode must update root and workspace Cargo versions"
test -z "$(git -C "$tmp" tag)" || fail "prepare mode must not create a tag"
if git -C "$tmp" log -1 --format=%s >/dev/null 2>&1; then
	fail "prepare mode must not create a commit"
fi
( cd "$tmp" && bun bump-version.ts patch --dry-run )
require_text "$tmp/Cargo.toml" 'version = "1.3.0"'

echo "distribution contracts: ok"
