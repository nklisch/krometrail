#!/usr/bin/env bash
# Static contract tests for release assets, installation, and version ownership.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE="$ROOT/.github/workflows/release.yml"
CI="$ROOT/.github/workflows/ci.yml"
PAGES="$ROOT/.github/workflows/deploy-pages.yml"
INSTALLER="$ROOT/scripts/install.sh"
DEV_INSTALLER="$ROOT/scripts/dev-install.sh"
INSTALLER_FIXTURES="$ROOT/tests/installer-fixtures.sh"
PLUGIN_BOOTSTRAP_FIXTURES="$ROOT/tests/plugin-bootstrap-fixtures.sh"
PLUGIN_STATIC="$ROOT/tests/plugin-static.sh"
VALIDATE="$ROOT/scripts/validate-release-tag.sh"
VERIFY_TAG="$ROOT/scripts/verify-release-tag-identity.sh"
BUMP="$ROOT/scripts/bump-version.ts"
PACKAGE="$ROOT/package.json"
CARGO="$ROOT/Cargo.toml"
CROSS_CONFIG="$ROOT/Cross.toml"

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

# Linux release rows must be target-triple explicit musl builds. A GNU target
# would make the binary's glibc minimum depend on the runner image, so reject
# both the old rows and any future reintroduction of rolling-runner output.
require_text "$RELEASE" 'target: x86_64-unknown-linux-musl'
require_text "$RELEASE" 'target: aarch64-unknown-linux-musl'
require_text "$RELEASE" 'runner: ubuntu-24.04'
if grep -Fq -- 'target: x86_64-unknown-linux-gnu' "$RELEASE" || \
	grep -Fq -- 'target: aarch64-unknown-linux-gnu' "$RELEASE"; then
	fail "Linux release assets must not use GNU rolling-runner targets"
fi

# The cross-build and smoke-test contracts are release gates, not advisory
# comments: every matrix artifact must execute --version before attestation and
# upload, with explicit arm64 emulation when native capacity is unavailable.
require_text "$RELEASE" 'houseabsolute/actions-rust-cross@21b0f18dc621b25bfae556ff2791fca4173121e8'
require_text "$RELEASE" 'cross-version: v0.2.5'
require_text "$CROSS_CONFIG" '[target.x86_64-unknown-linux-musl]'
require_text "$CROSS_CONFIG" '[target.aarch64-unknown-linux-musl]'
require_text "$CROSS_CONFIG" 'ghcr.io/cross-rs/x86_64-unknown-linux-musl@sha256:'
require_text "$CROSS_CONFIG" 'ghcr.io/cross-rs/aarch64-unknown-linux-musl@sha256:'
require_text "$RELEASE" 'docker/setup-qemu-action@c7c53464625b32c7a7e944ae62b3e17d2b600130'
require_text "$RELEASE" 'Smoke-test release asset in matching architecture'
require_text "$RELEASE" 'linux/amd64'
require_text "$RELEASE" 'linux/arm64/v8'
require_text "$RELEASE" 'docker run --rm --platform'
require_text "$RELEASE" '/dist/${{ matrix.asset }}" --version'
require_text "$RELEASE" '"./dist/${{ matrix.asset }}" --version'
smoke_line="$(grep -nF 'Smoke-test release asset in matching architecture' "$RELEASE" | cut -d: -f1)"
attest_line="$(grep -nF 'name: Attest release asset' "$RELEASE" | cut -d: -f1)"
upload_line="$(grep -nF 'name: Upload release asset' "$RELEASE" | cut -d: -f1)"
test -n "$smoke_line" && test -n "$attest_line" && test -n "$upload_line" || fail "release smoke/attestation/upload steps are incomplete"
test "$smoke_line" -lt "$attest_line" || fail "release asset smoke test must precede attestation"
test "$attest_line" -lt "$upload_line" || fail "release asset attestation must precede upload"

require_text "$RELEASE" "sha256sum"
require_text "$RELEASE" "dist/checksums.txt"
require_text "$RELEASE" "actions/attest-build-provenance@e8998f949152b193b063cb0ec769d69d929409be"
require_text "$RELEASE" "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02"
require_text "$RELEASE" "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093"

# Every release workflow action is immutable. The inline comments retain the
# human-readable reviewed channel while the full SHA is the executable ref.
while IFS= read -r uses_line; do
	uses_ref="${uses_line##*@}"
	uses_ref="${uses_ref%%[[:space:]]*}"
	if [[ ! "$uses_ref" =~ ^[0-9a-f]{40}$ ]]; then
		fail "release action is not pinned to a full commit SHA: $uses_line"
	fi
done < <(grep -E '^[[:space:]]*uses:' "$RELEASE")

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
require_text "$CI" 'rust-msrv:'
require_text "$CI" 'dtolnay/rust-toolchain@1.85.0'
require_text "$CI" 'name: Rust quality gate'
require_text "$CI" 'bash tests/distribution-static.sh'
require_text "$PAGES" 'bun install --frozen-lockfile'
require_text "$ROOT/docs/guide/development.md" 'bun install --frozen-lockfile'

require_text "$DEV_INSTALLER" 'CARGO_TARGET_DIR=target cargo build --locked --release'
if grep -Fq -- 'if [[ ! -f "$BINARY" ]]' "$DEV_INSTALLER"; then
	fail "developer installation must always rebuild the release binary"
fi
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
if git ls-files --error-unmatch bun.lock >/dev/null 2>&1; then :; else
	fail "bun.lock must be committed for reproducible documentation installs"
fi
if grep -Fq -- 'bun.lock' "$ROOT/.gitignore"; then
	fail ".gitignore must not ignore the committed bun.lock"
fi
if grep -Fq -- 'tests/agent-harness' "$ROOT/.gitignore"; then
	fail ".gitignore must not retain deleted agent-harness rules"
fi
if grep -Eiq -- 'npm[[:space:]]+publish|bun[[:space:]]+run[[:space:]]+(build|test)' "$PACKAGE"; then
	fail "package.json must not expose product build, test, or publish scripts"
fi
if grep -Eiq -- 'bun[[:space:]]+run[[:space:]]+build|npm[[:space:]]+publish' "$RELEASE"; then
	fail "release workflow must not build or publish the Bun package"
fi
require_text "$RELEASE" 'validate-release-tag'
require_text "$RELEASE" 'needs: validate-release-tag'
require_text "$RELEASE" 'needs: [validate-release-tag, build]'
require_text "$RELEASE" 'outputs:'
require_text "$RELEASE" 'tag_sha:'
require_text "$RELEASE" 'ref: ${{ needs.validate-release-tag.outputs.tag_sha }}'
require_text "$RELEASE" 'RELEASE_SHA:'
require_text "$RELEASE" 'git rev-parse HEAD'
require_text "$RELEASE" 'verify-release-tag-identity.sh'
require_text "$RELEASE" 'Verify publication tag identity after upload'
require_text "$RELEASE" 'workflow_dispatch:'
require_text "$BUMP" '["cargo", "update", "-p"'
require_text "$BUMP" '--precise'
require_text "$BUMP" '--locked'
require_text "$BUMP" 'packageMultiset'
require_text "$BUMP" 'source'
require_text "$BUMP" 'checksum'
require_text "$BUMP" 'sameMultiset'

# Validate exact tag refs against the manifest at the tagged commit. A branch
# with a release-shaped name is not a substitute for an existing tag.
tag_tmp="$(mktemp -d)"
cleanup_tag() { rm -rf "$tag_tmp"; }
trap cleanup_tag EXIT
cat > "$tag_tmp/Cargo.toml" <<'EOF'
[package]
name = "tag-fixture"
version = "1.2.3"
EOF
git -C "$tag_tmp" init -q
git -C "$tag_tmp" config user.email test@example.invalid
git -C "$tag_tmp" config user.name distribution-test
git -C "$tag_tmp" add Cargo.toml
git -C "$tag_tmp" commit -q -m tagged
git -C "$tag_tmp" tag v1.2.3
tag_sha="$(cd "$tag_tmp" && bash "$VALIDATE" v1.2.3)"
test "$tag_sha" = "$(git -C "$tag_tmp" rev-parse HEAD)" || fail "tag validator must print the exact tag commit"
if ( cd "$tag_tmp" && bash "$VALIDATE" v1.2.4 ) >/dev/null 2>&1; then
	fail "release tag validation must reject a branch/tag mismatch"
fi
if ( cd "$tag_tmp" && bash "$VALIDATE" v1.2.3-alpha ) >/dev/null 2>&1; then
	fail "release tag validation must reject prerelease tags"
fi
# A release-shaped branch without the exact tag must never be accepted.
git -C "$tag_tmp" branch v1.2.4
if ( cd "$tag_tmp" && bash "$VALIDATE" v1.2.4 ) >/dev/null 2>&1; then
	fail "release tag validation must reject a release-shaped branch"
fi
# An annotated tag is valid too, but its manifest and commit are still read
# through the exact refs/tags name rather than from the similarly named branch.
sed -i 's/version = "1.2.3"/version = "1.2.4"/' "$tag_tmp/Cargo.toml"
git -C "$tag_tmp" add Cargo.toml
git -C "$tag_tmp" commit -q -m annotated
git -C "$tag_tmp" tag -a -m annotated v1.2.4
annotated_sha="$(cd "$tag_tmp" && bash "$VALIDATE" v1.2.4)"
test "$annotated_sha" = "$(git -C "$tag_tmp" rev-parse HEAD)" || fail "annotated tag validator must dereference its commit"

# Verify publication identity for both lightweight and annotated tags.
( cd "$tag_tmp" && bash "$VERIFY_TAG" v1.2.3 "$tag_sha" . )
( cd "$tag_tmp" && bash "$VERIFY_TAG" v1.2.4 "$annotated_sha" . )
wrong_tag_sha="$(printf '%040d' 0)"
if ( cd "$tag_tmp" && bash "$VERIFY_TAG" v1.2.3 "$wrong_tag_sha" . ) >/dev/null 2>&1; then
	fail "publication identity verification must reject the wrong SHA"
fi
rm -rf "$tag_tmp"
trap - EXIT

# A stale target binary must be replaced even when it already exists. Run the
# entire fixture from its temporary repository and prove the real target output
# was not touched by the fake Cargo builder.
stale_tmp="$(mktemp -d)"
repo_binary="$ROOT/target/release/krometrail"
repo_snapshot="$stale_tmp/repository-krometrail-before"
repo_had_binary=false
if [[ -f "$repo_binary" ]]; then
	repo_had_binary=true
	cp "$repo_binary" "$repo_snapshot"
fi
mkdir -p "$stale_tmp/bin" "$stale_tmp/target/release" "$stale_tmp/home"
printf '#!/usr/bin/env bash\necho stale-binary\n' > "$stale_tmp/target/release/krometrail"
chmod +x "$stale_tmp/target/release/krometrail"
cat > "$stale_tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" > "$CARGO_TEST_LOG"
mkdir -p target/release
cat > target/release/krometrail <<'BINARY'
#!/usr/bin/env bash
echo krometrail-current
BINARY
chmod +x target/release/krometrail
EOF
chmod +x "$stale_tmp/bin/cargo"
(
	cd "$stale_tmp"
	CARGO_TEST_LOG="$stale_tmp/cargo.log" PATH="$stale_tmp/bin:$PATH" HOME="$stale_tmp/home" KROMETRAIL_INSTALL_DIR="$stale_tmp/install" \
		bash "$DEV_INSTALLER" > "$stale_tmp/install.log"
)
require_text "$stale_tmp/cargo.log" 'build --locked --release'
require_text "$stale_tmp/install.log" 'krometrail-current'
if $repo_had_binary; then
	cmp -s "$repo_binary" "$repo_snapshot" || fail "isolated developer install changed repository target/release/krometrail"
elif [[ -e "$repo_binary" ]]; then
	fail "isolated developer install created repository target/release/krometrail"
fi
rm -rf "$stale_tmp"

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
test "$(grep -Ec '^name = "version-bump-fixture"$' "$tmp/Cargo.lock")" -eq 1 || fail "prepare mode must refresh the Cargo lockfile"
test "$(grep -Ec '^version = "1\.3\.0"$' "$tmp/Cargo.lock")" -eq 1 || fail "prepare mode must refresh only the workspace package version"
test -z "$(git -C "$tmp" tag)" || fail "prepare mode must not create a tag"
if git -C "$tmp" log -1 --format=%s >/dev/null 2>&1; then
	fail "prepare mode must not create a commit"
fi
( cd "$tmp" && bun bump-version.ts patch --dry-run )
require_text "$tmp/Cargo.toml" 'version = "1.3.0"'

# Krometrail releases derive plugin metadata from Cargo's sole version authority.
# Exercise successful projection and all-file rollback with a fake Cargo runner.
plugin_version_tmp="$(mktemp -d)"
mkdir -p "$plugin_version_tmp/plugin/.claude-plugin" "$plugin_version_tmp/plugin/.codex-plugin" \
  "$plugin_version_tmp/.claude-plugin" "$plugin_version_tmp/.agents/plugins" "$plugin_version_tmp/bin"
cat >"$plugin_version_tmp/Cargo.toml" <<'EOF'
[package]
name = "krometrail"
version = "1.2.3"
edition = "2024"

[workspace.package]
version = "1.2.3"
EOF
cat >"$plugin_version_tmp/Cargo.lock" <<'EOF'
# fixture lock
version = 4

[[package]]
name = "krometrail"
version = "1.2.3"
EOF
for manifest in \
  plugin/.claude-plugin/plugin.json \
  plugin/.codex-plugin/plugin.json \
  .claude-plugin/marketplace.json \
  .agents/plugins/marketplace.json; do
  printf '{"name":"krometrail","version":"1.2.3"}\n' >"$plugin_version_tmp/$manifest"
done
printf '1.2.3\n' >"$plugin_version_tmp/plugin/version"
cat >"$plugin_version_tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == update ]]; then
  sed '0,/version = "1.2.3"/s//version = "1.2.4"/' Cargo.lock >Cargo.lock.next
  mv Cargo.lock.next Cargo.lock
elif [[ "${PLUGIN_VERSION_FAIL:-0}" == 1 && "${1:-}" == check ]]; then
  exit 17
fi
EOF
chmod +x "$plugin_version_tmp/bin/cargo"
cp "$BUMP" "$plugin_version_tmp/bump-version.ts"
(
  cd "$plugin_version_tmp"
  PATH="$plugin_version_tmp/bin:$PATH" bun bump-version.ts patch --prepare
)
test "$(grep -Ec '^version = "1\.2\.4"$' "$plugin_version_tmp/Cargo.toml")" -eq 2 || fail "plugin prepare did not update Cargo versions"
for manifest in \
  plugin/.claude-plugin/plugin.json \
  plugin/.codex-plugin/plugin.json \
  .claude-plugin/marketplace.json \
  .agents/plugins/marketplace.json; do
  require_text "$plugin_version_tmp/$manifest" '"version":"1.2.4"'
done
test "$(cat "$plugin_version_tmp/plugin/version")" = "1.2.4" || fail "plugin prepare did not update the launcher version"

# Restore the fixture, force a release-gate failure, and prove every projection rolls back.
sed -i 's/1\.2\.4/1.2.3/g' "$plugin_version_tmp/Cargo.toml" "$plugin_version_tmp/Cargo.lock" \
  "$plugin_version_tmp/plugin/.claude-plugin/plugin.json" \
  "$plugin_version_tmp/plugin/.codex-plugin/plugin.json" \
  "$plugin_version_tmp/.claude-plugin/marketplace.json" \
  "$plugin_version_tmp/.agents/plugins/marketplace.json" \
  "$plugin_version_tmp/plugin/version"
cp -R "$plugin_version_tmp" "$plugin_version_tmp.before"
if (
  cd "$plugin_version_tmp"
  PLUGIN_VERSION_FAIL=1 PATH="$plugin_version_tmp/bin:$PATH" bun bump-version.ts patch --prepare
) >/dev/null 2>&1; then
  fail "plugin version projection accepted a failed release gate"
fi
for file in \
  Cargo.toml Cargo.lock plugin/version \
  plugin/.claude-plugin/plugin.json plugin/.codex-plugin/plugin.json \
  .claude-plugin/marketplace.json .agents/plugins/marketplace.json; do
  cmp -s "$plugin_version_tmp/$file" "$plugin_version_tmp.before/$file" || fail "failed release did not restore $file"
done
rm -rf "$plugin_version_tmp" "$plugin_version_tmp.before"

# Exercise lock refresh validation with duplicate package names. The fake Cargo
# command keeps this test hermetic while the bump helper still performs its
# real parser, multiset comparison, and rollback behavior.
lock_tmp="$(mktemp -d)"
cat > "$lock_tmp/Cargo.toml" <<'EOF'
[package]
name = "lock-fixture"
version = "2.0.0"
edition = "2024"
EOF
cat > "$lock_tmp/Cargo.lock" <<'EOF'
# fixture lock
version = 4

[[package]]
name = "lock-fixture"
version = "2.0.0"

[[package]]
name = "duplicate"
version = "1.0.0"
source = "git+https://example.invalid/one"
checksum = "checksum-one"

[[package]]
name = "duplicate"
version = "1.0.0"
source = "registry+https://example.invalid/index"
checksum = "checksum-two"
EOF
cat > "$lock_tmp/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == update ]]; then
	sed '0,/version = "2.0.0"/s//version = "2.0.1"/' Cargo.lock > Cargo.lock.next
	if [[ "${LOCK_MODE:-positive}" == negative ]]; then
		sed '0,/checksum-one/s//checksum-changed/' Cargo.lock.next > Cargo.lock
		rm Cargo.lock.next
	else
		mv Cargo.lock.next Cargo.lock
	fi
fi
EOF
chmod +x "$lock_tmp/cargo"
cp "$BUMP" "$lock_tmp/bump-version.ts"
# The fixture's original lock has two distinct duplicate-name records; an
# unchanged refresh must pass and retain both records.
(
	cd "$lock_tmp"
	LOCK_MODE=positive PATH="$lock_tmp:$PATH" bun bump-version.ts patch --prepare
)
test "$(grep -Ec '^name = "duplicate"$' "$lock_tmp/Cargo.lock")" -eq 2 || fail "lock multiset positive fixture lost a duplicate package"
test "$(grep -Ec '^version = "2\.0\.1"$' "$lock_tmp/Cargo.lock")" -eq 1 || fail "lock multiset positive fixture did not refresh workspace version"
# Restore the original fixture, then mutate one duplicate's checksum. The
# helper must reject the change and restore both Cargo files without tagging.
cat > "$lock_tmp/Cargo.toml" <<'EOF'
[package]
name = "lock-fixture"
version = "2.0.0"
edition = "2024"
EOF
cat > "$lock_tmp/Cargo.lock" <<'EOF'
# fixture lock
version = 4

[[package]]
name = "lock-fixture"
version = "2.0.0"

[[package]]
name = "duplicate"
version = "1.0.0"
source = "git+https://example.invalid/one"
checksum = "checksum-one"

[[package]]
name = "duplicate"
version = "1.0.0"
source = "registry+https://example.invalid/index"
checksum = "checksum-two"
EOF
cp "$lock_tmp/Cargo.toml" "$lock_tmp/Cargo.toml.before"
cp "$lock_tmp/Cargo.lock" "$lock_tmp/Cargo.lock.before"
if ( cd "$lock_tmp" && LOCK_MODE=negative PATH="$lock_tmp:$PATH" bun bump-version.ts patch --prepare ) >/dev/null 2>&1; then
	fail "lock multiset negative fixture accepted a duplicate package mutation"
fi
cmp -s "$lock_tmp/Cargo.toml" "$lock_tmp/Cargo.toml.before" || fail "failed lock validation did not restore Cargo.toml"
cmp -s "$lock_tmp/Cargo.lock" "$lock_tmp/Cargo.lock.before" || fail "failed lock validation did not restore Cargo.lock"
rm -rf "$lock_tmp"

bash "$INSTALLER_FIXTURES"
bash "$PLUGIN_BOOTSTRAP_FIXTURES"
bash "$PLUGIN_STATIC"

echo "distribution contracts: ok"
