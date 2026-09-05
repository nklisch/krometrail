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
OWNERSHIP="$ROOT/scripts/release-ownership.ts"
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

# The release-ownership inventory is the single registry of shipped version
# projections. Fixtures generate files and assert outcomes from the same
# exports the release helper consumes, so a registry change cannot outrun the
# tests or vice versa.
list_product_projections() {
	bun -e 'const m = await import(process.argv[1]); for (const p of m.PRODUCT_VERSION_PROJECTIONS) console.log(`${p.path}\t${p.format}`);' "$OWNERSHIP"
}

write_projection_files() {
	local dir="$1"
	local version="$2"
	local path format
	while IFS=$'\t' read -r path format; do
		mkdir -p "$(dirname "$dir/$path")"
		if [[ "$format" == "text" ]]; then
			printf '%s\n' "$version" >"$dir/$path"
		else
			printf '{"name":"krometrail","version":"%s"}\n' "$version" >"$dir/$path"
		fi
	done < <(list_product_projections)
}

assert_projections_at() {
	local dir="$1"
	local version="$2"
	bun -e 'const m = await import(process.argv[1]); await m.assertProjectionsAtVersion(process.argv[2], process.argv[3]);' \
		"$OWNERSHIP" "$dir" "$version" || fail "version projections under $dir do not equal $version"
}

# Snapshot and byte-compare a whole fixture repo so a scenario that must not
# mutate anything (or must roll everything back) is checked file-for-file. The
# clean fixture baseline and per-scenario snapshots are separate directories:
# a scenario snapshot must never become another scenario's restore source.
snapshot_repo() {
	local repo="$1"
	local snapshot="$2"
	rm -rf "$snapshot"
	cp -R "$repo" "$snapshot"
}

assert_repo_unchanged() {
	local repo="$1"
	local snapshot="$2"
	local scenario="$3"
	diff -r "$repo" "$snapshot" >/dev/null || fail "$scenario changed files that must stay byte-identical"
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
# Parse the actual YAML/TOML and exercise negative toolchain-selection mutations.
bun test "$ROOT/tests/minimum-rust-workflow.test.ts"
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
require_text "$RELEASE" 'Verify publication tag identity before publish'
require_text "$RELEASE" 'Verify publication tag identity after publish'
require_text "$RELEASE" 'workflow_dispatch:'
# The release helper must drive every shipped version projection from the one
# ownership inventory; a second projection list would let releases drift.
test -f "$OWNERSHIP" || fail "scripts/release-ownership.ts inventory module is missing"
require_text "$BUMP" 'from "./release-ownership.ts"'
require_text "$BUMP" 'findUnregisteredVersionProjections'
require_text "$BUMP" 'independentMemberNames'
require_text "$BUMP" 'inheritsWorkspaceVersion'
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
cp "$BUMP" "$OWNERSHIP" "$tmp/"
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

# Krometrail releases derive every registered plugin/catalog projection from
# Cargo's sole version authority. Fixture files are generated from the same
# inventory the helper consumes. Exercise successful projection and all-file
# rollback with a fake Cargo runner.
plugin_version_tmp="$(mktemp -d)"
mkdir -p "$plugin_version_tmp/bin"
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
write_projection_files "$plugin_version_tmp" "1.2.3"
cat >"$plugin_version_tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == update ]]; then
  sed '0,/version = "1.2.3"/s//version = "1.2.4"/' Cargo.lock >Cargo.lock.next
  mv Cargo.lock.next Cargo.lock
elif [[ "${BUMP_GATE_FAIL:-0}" == 1 && "${1:-}" == check ]]; then
  exit 17
fi
EOF
chmod +x "$plugin_version_tmp/bin/cargo"
cp "$BUMP" "$OWNERSHIP" "$plugin_version_tmp/"
snapshot_repo "$plugin_version_tmp" "$plugin_version_tmp.pristine"
(
  cd "$plugin_version_tmp"
  PATH="$plugin_version_tmp/bin:$PATH" bun bump-version.ts patch --prepare
)
test "$(grep -Ec '^version = "1\.2\.4"$' "$plugin_version_tmp/Cargo.toml")" -eq 2 || fail "plugin prepare did not update Cargo versions"
assert_projections_at "$plugin_version_tmp" "1.2.4"

# Force a release-gate failure from the pristine fixture and prove every file
# — Cargo metadata, lockfile, and each registered projection — rolls back. The
# exact gate error proves the failure was the injected gate, not an earlier
# refusal.
rm -rf "$plugin_version_tmp"
cp -R "$plugin_version_tmp.pristine" "$plugin_version_tmp"
if (
  cd "$plugin_version_tmp"
  BUMP_GATE_FAIL=1 PATH="$plugin_version_tmp/bin:$PATH" bun bump-version.ts patch --prepare
) >"$plugin_version_tmp.out" 2>&1; then
  fail "plugin version projection accepted a failed release gate"
fi
grep -Fq 'Command failed (17): cargo check' "$plugin_version_tmp.out" \
	|| fail "plugin rollback fixture did not fail at the injected release gate"
assert_repo_unchanged "$plugin_version_tmp" "$plugin_version_tmp.pristine" "failed plugin release"
rm -rf "$plugin_version_tmp" "$plugin_version_tmp.pristine" "$plugin_version_tmp.out"

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
cp "$BUMP" "$OWNERSHIP" "$lock_tmp/"
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


# The real workspace mixes product-owned crates with independently versioned
# ones. This hermetic mirror proves a product bump refreshes exactly the
# product-owned lock entries while the independent crate and unrelated
# third-party entries stay byte-identical, that every registered projection
# moves with the release, and that inconsistent or unregistered shipped
# version surfaces are refused before any mutation.
mixed_tmp="$(mktemp -d)"
mkdir -p "$mixed_tmp/crates/owned-member" "$mixed_tmp/crates/independent-member" \
	"$mixed_tmp/crates/single-quoted-member" "$mixed_tmp/crates/omitted-version-member" "$mixed_tmp/bin"
cat >"$mixed_tmp/Cargo.toml" <<'EOF'
[package]
name = "krometrail"
version = "1.6.2"
edition = "2024"

[workspace]
resolver = "2"
members = ["crates/owned-member", "crates/independent-member", "crates/single-quoted-member", "crates/omitted-version-member"]

[workspace.package]
version = "1.6.2"
EOF
cat >"$mixed_tmp/crates/owned-member/Cargo.toml" <<'EOF'
[package]
name = "owned-member"
version.workspace = true
edition = "2024"
EOF
cat >"$mixed_tmp/crates/independent-member/Cargo.toml" <<'EOF'
[package]
name = "independent-member"
version = "0.1.1"
edition = "2024"
EOF
cat >"$mixed_tmp/crates/single-quoted-member/Cargo.toml" <<'EOF'
[package]
name = "single-quoted-member"
version = '0.2.2'
edition = "2024"
EOF
cat >"$mixed_tmp/crates/omitted-version-member/Cargo.toml" <<'EOF'
[package]
name = "omitted-version-member"
edition = "2024"
EOF
cat >"$mixed_tmp/Cargo.lock" <<'EOF'
# fixture lock
version = 4

[[package]]
name = "krometrail"
version = "1.6.2"

[[package]]
name = "owned-member"
version = "1.6.2"

[[package]]
name = "independent-member"
version = "0.1.1"

[[package]]
name = "single-quoted-member"
version = "0.2.2"

[[package]]
name = "omitted-version-member"
version = "0.0.0"

[[package]]
name = "unrelated-dep"
version = "5.0.3"
source = "registry+https://example.invalid/index"
checksum = "checksum-unrelated"
EOF
write_projection_files "$mixed_tmp" "1.6.2"
cat >"$mixed_tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
# Models the real `cargo update -p krometrail --precise X` projection: the
# product entry and every version-inheriting member move; the independently
# versioned member and third-party entries stay byte-identical. If this fake
# ever overreaches, the verifier's independent-entry and multiset checks fail
# the fixture rather than hiding the overreach. Every invocation is traced so
# scenarios can prove which release steps actually ran.
set -euo pipefail
printf '%s\n' "$*" >>"${CARGO_TRACE_FILE:-/dev/null}"
if [[ "${1:-}" == update ]]; then
  printf 'update-manifest-at=%s\n' "$(grep -m1 '^version = ' Cargo.toml)" >>"${CARGO_TRACE_FILE:-/dev/null}"
  new_version="${5:?expected update -p <name> --precise <version>}"
  awk -v new_version="$new_version" '
    /^\[\[package\]\][[:space:]]*$/ {
      if (active) printf "%s", cache
      cache = $0 "\n"; active = 1; name = ""; next
    }
    active && /^name = / { name = $0 }
    {
      if (active && (name == "name = \"krometrail\"" || name == "name = \"owned-member\"") && /^version = /)
        sub(/version = "[^"]*"/, "version = \"" new_version "\"")
      if (active) cache = cache $0 "\n"; else print
      next
    }
    END { if (active) printf "%s", cache }
  ' Cargo.lock >Cargo.lock.next
  mv Cargo.lock.next Cargo.lock
elif [[ "${BUMP_GATE_FAIL:-0}" == 1 && "${1:-}" == check ]]; then
  exit 17
fi
EOF
chmod +x "$mixed_tmp/bin/cargo"
cp "$BUMP" "$OWNERSHIP" "$mixed_tmp/"
# The clean fixture state is snapshotted once and never replaced: every
# scenario restores from this baseline, and refusal/rollback checks compare
# against their own per-scenario snapshot instead.
snapshot_repo "$mixed_tmp" "$mixed_tmp.baseline"

# A dry run validates the registered projections, invokes no release step, and
# mutates nothing.
( cd "$mixed_tmp" && CARGO_TRACE_FILE="$mixed_tmp.trace" PATH="$mixed_tmp/bin:$PATH" bun bump-version.ts patch --dry-run ) >"$mixed_tmp.out" 2>&1 \
	|| fail "dry run rejected the mixed-version workspace"
if [[ -s "$mixed_tmp.trace" ]]; then
	fail "dry run invoked cargo"
fi
assert_repo_unchanged "$mixed_tmp" "$mixed_tmp.baseline" "dry run"
rm -f "$mixed_tmp.trace" "$mixed_tmp.out"

# A successful prepare refreshes exactly the product-owned lock entries and
# every registered projection, and never touches the independent crate.
( cd "$mixed_tmp" && CARGO_TRACE_FILE="$mixed_tmp.trace" PATH="$mixed_tmp/bin:$PATH" bun bump-version.ts patch --prepare ) >"$mixed_tmp.out" 2>&1 \
	|| fail "mixed-version workspace prepare failed"
grep -Fq 'update-manifest-at=version = "1.6.3"' "$mixed_tmp.trace" \
	|| fail "mixed prepare did not rewrite the manifest before the lock refresh"
test "$(grep -Ec '^version = "1\.6\.3"$' "$mixed_tmp/Cargo.toml")" -eq 2 || fail "mixed prepare did not update root and workspace Cargo versions"
test "$(grep -A1 '^name = "krometrail"$' "$mixed_tmp/Cargo.lock" | grep -c '^version = "1\.6\.3"$')" -eq 1 || fail "mixed prepare did not refresh the product lock entry"
test "$(grep -A1 '^name = "owned-member"$' "$mixed_tmp/Cargo.lock" | grep -c '^version = "1\.6\.3"$')" -eq 1 || fail "mixed prepare did not refresh the version-inheriting member lock entry"
test "$(grep -A1 '^name = "independent-member"$' "$mixed_tmp/Cargo.lock" | grep -c '^version = "0\.1\.1"$')" -eq 1 || fail "mixed prepare moved the independent member lock entry"
test "$(grep -A1 '^name = "single-quoted-member"$' "$mixed_tmp/Cargo.lock" | grep -c '^version = "0\.2\.2"$')" -eq 1 || fail "mixed prepare moved the single-quoted member lock entry"
test "$(grep -A1 '^name = "omitted-version-member"$' "$mixed_tmp/Cargo.lock" | grep -c '^version = "0\.0\.0"$')" -eq 1 || fail "mixed prepare moved the omitted-version member lock entry"
test "$(grep -Ec '^version = "0\.1\.1"$' "$mixed_tmp/crates/independent-member/Cargo.toml")" -eq 1 || fail "mixed prepare moved the independent member manifest"
test "$(grep -Ec "^version = '0\\.2\\.2'$" "$mixed_tmp/crates/single-quoted-member/Cargo.toml")" -eq 1 || fail "mixed prepare moved the single-quoted member manifest"
test "$(grep -Ec '^version' "$mixed_tmp/crates/omitted-version-member/Cargo.toml")" -eq 0 || fail "mixed prepare added a version to the omitted-version member manifest"
test "$(grep -A1 '^name = "unrelated-dep"$' "$mixed_tmp/Cargo.lock" | grep -c '^version = "5\.0\.3"$')" -eq 1 || fail "mixed prepare moved an unrelated lock entry"
assert_projections_at "$mixed_tmp" "1.6.3"
rm -f "$mixed_tmp.trace" "$mixed_tmp.out"

# An unregistered version-bearing file in the shipped surface must be refused
# before any mutation: a new projection can never be silently skipped.
rm -rf "$mixed_tmp"
cp -R "$mixed_tmp.baseline" "$mixed_tmp"
printf '{"name":"krometrail","version":"1.6.2"}\n' >"$mixed_tmp/plugin/unregistered.json"
snapshot_repo "$mixed_tmp" "$mixed_tmp.scenario"
if ( cd "$mixed_tmp" && CARGO_TRACE_FILE="$mixed_tmp.trace" PATH="$mixed_tmp/bin:$PATH" bun bump-version.ts patch --prepare ) >"$mixed_tmp.out" 2>&1; then
	fail "bump accepted an unregistered shipped version projection"
fi
grep -Fq 'Unregistered shipped version projection(s): plugin/unregistered.json' "$mixed_tmp.out" \
	|| fail "unregistered projection refusal lost its named diagnostic"
if [[ -s "$mixed_tmp.trace" ]]; then
	fail "unregistered projection refusal invoked cargo"
fi
assert_repo_unchanged "$mixed_tmp" "$mixed_tmp.scenario" "unregistered projection rejection"
rm -f "$mixed_tmp.trace" "$mixed_tmp.out"

# A registered projection that does not carry the current product version is
# an inconsistent product-owned input and must be rejected before mutation.
rm -rf "$mixed_tmp"
cp -R "$mixed_tmp.baseline" "$mixed_tmp"
printf '{"name":"krometrail","version":"9.9.9"}\n' >"$mixed_tmp/.claude-plugin/marketplace.json"
snapshot_repo "$mixed_tmp" "$mixed_tmp.scenario"
if ( cd "$mixed_tmp" && CARGO_TRACE_FILE="$mixed_tmp.trace" PATH="$mixed_tmp/bin:$PATH" bun bump-version.ts patch --prepare ) >"$mixed_tmp.out" 2>&1; then
	fail "bump accepted a drifted registered projection"
fi
grep -Fq '.claude-plugin/marketplace.json must contain exactly one version equal to 1.6.2' "$mixed_tmp.out" \
	|| fail "drifted projection refusal was not the version validation error"
if [[ -s "$mixed_tmp.trace" ]]; then
	fail "drifted projection refusal invoked cargo"
fi
assert_repo_unchanged "$mixed_tmp" "$mixed_tmp.scenario" "drifted projection rejection"
rm -f "$mixed_tmp.trace" "$mixed_tmp.out"

# A failed release gate rolls back every mutated file, including the lockfile
# and all registered projections. The trace proves the run actually reached
# the release steps: the manifest was rewritten before the lock refresh, and
# the injected gate is the exact failure.
rm -rf "$mixed_tmp"
cp -R "$mixed_tmp.baseline" "$mixed_tmp"
snapshot_repo "$mixed_tmp" "$mixed_tmp.scenario"
if (
	cd "$mixed_tmp"
	BUMP_GATE_FAIL=1 CARGO_TRACE_FILE="$mixed_tmp.trace" PATH="$mixed_tmp/bin:$PATH" bun bump-version.ts patch --prepare
) >"$mixed_tmp.out" 2>&1; then
	fail "mixed-version prepare accepted a failed release gate"
fi
grep -Fq 'update -p krometrail --precise 1.6.3' "$mixed_tmp.trace" \
	|| fail "gate-fail scenario never ran the lock refresh"
grep -Fq 'update-manifest-at=version = "1.6.3"' "$mixed_tmp.trace" \
	|| fail "gate-fail scenario refreshed the lock before rewriting the manifest"
grep -Fxq 'check --workspace --all-targets --locked' "$mixed_tmp.trace" \
	|| fail "gate-fail scenario never reached the failing release gate"
grep -Fq 'Command failed (17): cargo check' "$mixed_tmp.out" \
	|| fail "gate-fail scenario was not the injected release gate failure"
assert_repo_unchanged "$mixed_tmp" "$mixed_tmp.scenario" "failed mixed-version release"
rm -rf "$mixed_tmp" "$mixed_tmp.baseline" "$mixed_tmp.scenario" "$mixed_tmp.trace" "$mixed_tmp.out"

# Recoupling the independent crate into the workspace version — in either TOML
# shape — must be refused by name before any mutation, even under --dry-run.
recouple_tmp="$(mktemp -d)"
mkdir -p "$recouple_tmp/crates/temporal-vision"
cat >"$recouple_tmp/Cargo.toml" <<'EOF'
[package]
name = "krometrail"
version = "1.6.2"
edition = "2024"

[workspace]
resolver = "2"
members = ["crates/temporal-vision"]

[workspace.package]
version = "1.6.2"
EOF
cat >"$recouple_tmp/crates/temporal-vision/Cargo.toml" <<'EOF'
[package]
name = "temporal-vision"
version.workspace = true
edition = "2024"
EOF
cp "$BUMP" "$OWNERSHIP" "$recouple_tmp/"
snapshot_repo "$recouple_tmp" "$recouple_tmp.pristine"
if ( cd "$recouple_tmp" && bun bump-version.ts patch --dry-run ) >"$recouple_tmp.out" 2>&1; then
	fail "bump accepted a dotted-form recoupled temporal-vision"
fi
grep -Fq 'crates/temporal-vision is versioned independently' "$recouple_tmp.out" \
	|| fail "dotted recoupling refusal lost its named diagnostic"
assert_repo_unchanged "$recouple_tmp" "$recouple_tmp.pristine" "dotted recoupling refusal"
printf '[package]\nname = "temporal-vision"\nversion = { workspace = true }\nedition = "2024"\n' >"$recouple_tmp/crates/temporal-vision/Cargo.toml"
snapshot_repo "$recouple_tmp" "$recouple_tmp.scenario"
if ( cd "$recouple_tmp" && bun bump-version.ts patch --dry-run ) >"$recouple_tmp.out" 2>&1; then
	fail "bump accepted an inline-form recoupled temporal-vision"
fi
grep -Fq 'crates/temporal-vision is versioned independently' "$recouple_tmp.out" \
	|| fail "inline recoupling refusal lost its named diagnostic"
assert_repo_unchanged "$recouple_tmp" "$recouple_tmp.scenario" "inline recoupling refusal"
rm -rf "$recouple_tmp" "$recouple_tmp.pristine" "$recouple_tmp.scenario" "$recouple_tmp.out"

# Prove the ownership model against real Cargo: `cargo update -p krometrail
# --precise` refreshes the product and version-inheriting member entries while
# the independent member keeps its own version. Lock resolution runs for real,
# offline, on this minimal workspace; the release gates are stubbed because
# compilation is not under test here.
real_cargo="$(command -v cargo)" || fail "cargo is required for the real lock-refresh fixture"
real_tmp="$(mktemp -d)"
mkdir -p "$real_tmp/crates/owned-dotted/src" "$real_tmp/crates/owned-inline/src" \
	"$real_tmp/crates/independent-single/src" "$real_tmp/crates/independent-omitted/src" \
	"$real_tmp/src" "$real_tmp/bin"
printf 'fn main() {}\n' >"$real_tmp/src/main.rs"
printf '' >"$real_tmp/crates/owned-dotted/src/lib.rs"
printf '' >"$real_tmp/crates/owned-inline/src/lib.rs"
printf '' >"$real_tmp/crates/independent-single/src/lib.rs"
printf '' >"$real_tmp/crates/independent-omitted/src/lib.rs"
cat >"$real_tmp/Cargo.toml" <<'EOF'
[package]
name = "krometrail"
version = "1.6.2"
edition = "2024"

[workspace]
resolver = "2"
members = ["crates/owned-dotted", "crates/owned-inline", "crates/independent-single", "crates/independent-omitted"]

[workspace.package]
version = "1.6.2"
EOF
cat >"$real_tmp/crates/owned-dotted/Cargo.toml" <<'EOF'
[package]
name = "owned-dotted"
version.workspace = true
edition = "2024"
EOF
cat >"$real_tmp/crates/owned-inline/Cargo.toml" <<'EOF'
[package]
name = "owned-inline"
version = { workspace = true }
edition = "2024"
EOF
cat >"$real_tmp/crates/independent-single/Cargo.toml" <<'EOF'
[package]
name = "independent-single"
version = '0.1.1'
edition = "2024"
EOF
cat >"$real_tmp/crates/independent-omitted/Cargo.toml" <<'EOF'
[package]
name = "independent-omitted"
edition = "2024"
EOF
write_projection_files "$real_tmp" "1.6.2"
cat >"$real_tmp/bin/cargo" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == update ]]; then
  exec "$real_cargo" "\$@"
fi
exit 0
EOF
chmod +x "$real_tmp/bin/cargo"
cp "$BUMP" "$OWNERSHIP" "$real_tmp/"
( cd "$real_tmp" && CARGO_NET_OFFLINE=1 "$real_cargo" generate-lockfile --offline ) >/dev/null \
	|| fail "real-cargo fixture could not generate its lockfile"
( cd "$real_tmp" && CARGO_NET_OFFLINE=1 PATH="$real_tmp/bin:$PATH" bun bump-version.ts patch --prepare ) >/dev/null \
	|| fail "real-cargo lock refresh failed"
test "$(grep -A1 '^name = "krometrail"$' "$real_tmp/Cargo.lock" | grep -c '^version = "1\.6\.3"$')" -eq 1 || fail "real cargo did not refresh the product lock entry"
test "$(grep -A1 '^name = "owned-dotted"$' "$real_tmp/Cargo.lock" | grep -c '^version = "1\.6\.3"$')" -eq 1 || fail "real cargo did not refresh the dotted-inheriting member lock entry"
test "$(grep -A1 '^name = "owned-inline"$' "$real_tmp/Cargo.lock" | grep -c '^version = "1\.6\.3"$')" -eq 1 || fail "real cargo did not refresh the inline-table-inheriting member lock entry"
test "$(grep -A1 '^name = "independent-single"$' "$real_tmp/Cargo.lock" | grep -c '^version = "0\.1\.1"$')" -eq 1 || fail "real cargo moved the single-quoted member lock entry"
test "$(grep -A1 '^name = "independent-omitted"$' "$real_tmp/Cargo.lock" | grep -c '^version = "0\.0\.0"$')" -eq 1 || fail "real cargo moved the omitted-version member lock entry"
test "$(grep -Ec "^version = '0\\.1\\.1'$" "$real_tmp/crates/independent-single/Cargo.toml")" -eq 1 || fail "real-cargo fixture moved the single-quoted member manifest"
test "$(grep -Ec '^version' "$real_tmp/crates/independent-omitted/Cargo.toml")" -eq 0 || fail "real-cargo fixture added a version to the omitted-version member manifest"
rm -rf "$real_tmp"

# The inventory must match the real shipped surface, not merely fixtures built
# from it. Byte-copy the real shipped directories — never symlinks, so a test
# write can never reach the worktree — and check the registry against them in
# both directions using the production scan itself.
surface_tmp="$(mktemp -d)"
mkdir -p "$surface_tmp/repo"
for surface in plugin .claude-plugin .agents/plugins; do
	mkdir -p "$surface_tmp/repo/$(dirname "$surface")"
	cp -R "$ROOT/$surface" "$surface_tmp/repo/$surface"
done
if find "$surface_tmp/repo" -type l | grep -q .; then
	fail "shipped-surface copy unexpectedly contains a symlink"
fi
cp "$OWNERSHIP" "$surface_tmp/release-ownership.ts"
bun -e '
import { existsSync } from "node:fs";
const m = await import(process.argv[1]);
const root = process.argv[2];
const missing = m.PRODUCT_VERSION_PROJECTIONS.filter((p) => !existsSync(`${root}/${p.path}`));
if (missing.length > 0) {
	console.error("registered projections missing from the repository: " + missing.map((p) => p.path).join(", "));
	process.exit(1);
}
' "$surface_tmp/release-ownership.ts" "$surface_tmp/repo" || fail "registered projection missing from copied shipped surface"
surface_scan="$(bun -e 'const m = await import(process.argv[1]); console.log(JSON.stringify(m.findUnregisteredVersionProjections(process.argv[2])));' "$surface_tmp/release-ownership.ts" "$surface_tmp/repo")"
test "$surface_scan" = "[]" || fail "real shipped surface carries unregistered version projection(s): $surface_scan"
# Mutation check: dropping a registry entry must be caught by the same scan
# against the same real files. The production registry is never modified; the
# mutated copy lives only inside this fixture directory.
for dropped in plugin/plugin.json .agents/plugins/marketplace.json; do
	grep -F -v "\"$dropped\"" "$surface_tmp/release-ownership.ts" >"$surface_tmp/release-ownership.mutated.ts"
	mutated_scan="$(bun -e 'const m = await import(process.argv[1]); console.log(JSON.stringify(m.findUnregisteredVersionProjections(process.argv[2])));' "$surface_tmp/release-ownership.mutated.ts" "$surface_tmp/repo")"
	test "$mutated_scan" = "[\"$dropped\"]" \
		|| fail "dropping $dropped was not caught by the shipped-surface scan: $mutated_scan"
done
rm -rf "$surface_tmp"

# The ownership predicate must positively recognize explicit workspace-version
# inheritance in every supported TOML shape, and must classify single-quoted
# literals and omitted versions (Cargo defaults them to 0.0.0) as independent.
bun -e '
const m = await import(process.argv[1]);
const apostrophe = String.fromCharCode(39);
const cases = [
	["[package]\nname = \"owned\"\nversion.workspace = true\n", true],
	["[package]\nname = \"owned\"\nversion = { workspace = true }\n", true],
	["[package]\nname = \"independent\"\nversion = \"0.1.1\"\n", false],
	["[package]\nname = \"independent\"\nversion = " + apostrophe + "0.1.1" + apostrophe + "\n", false],
	["[package]\nname = \"independent\"\n", false],
];
for (const [manifest, expected] of cases) {
	if (m.inheritsWorkspaceVersion(manifest) !== expected) {
		console.error("ownership predicate misclassified manifest:\n" + manifest);
		process.exit(1);
	}
}
' "$OWNERSHIP" || fail "ownership predicate misclassified a supported manifest shape"

# Releases must be created as a draft, verified complete, then published, so a
# consumer resolving an exact version never observes a partial asset set.
require_text "$RELEASE" 'draft: true'
require_text "$RELEASE" 'Verify draft carries the complete asset set'
require_text "$RELEASE" 'gh release edit "$RELEASE_TAG" --draft=false'
grep -q 'isDraft' "$RELEASE" || fail "release workflow must assert the draft state before publishing"

bash "$INSTALLER_FIXTURES"
bash "$PLUGIN_BOOTSTRAP_FIXTURES"
bash "$PLUGIN_STATIC"

echo "distribution contracts: ok"
