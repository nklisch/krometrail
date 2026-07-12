#!/usr/bin/env bash
set -euo pipefail

# Build the qualification binary in a disposable worktree, delete that worktree, and then use
# the cached binary against the surviving checkout. This catches accidental CARGO_MANIFEST_DIR
# dependencies that only fail after a shared CARGO_TARGET_DIR is reused.
repo_root="$(git rev-parse --show-toplevel)"
revision="$(git -C "$repo_root" rev-parse HEAD)"
worktree="$(mktemp -d "${TMPDIR:-/tmp}/krometrail-cdp-build-worktree.XXXXXX")"
target_dir="$(mktemp -d "${TMPDIR:-/tmp}/krometrail-cdp-shared-target.XXXXXX")"
output_dir="$(mktemp -d "${TMPDIR:-/tmp}/krometrail-cdp-attestation.XXXXXX")"

cleanup() {
	git -C "$repo_root" worktree remove --force "$worktree" >/dev/null 2>&1 || true
	rm -rf "$worktree" "$target_dir" "$output_dir"
}
trap cleanup EXIT

git -C "$repo_root" worktree add --detach "$worktree" "$revision" >/dev/null
CARGO_TARGET_DIR="$target_dir" cargo build --locked \
	--manifest-path "$worktree/Cargo.toml" \
	-p krometrail-cdp \
	--features cdp-spike-cdpkit \
	--bin cdp-transport-gate

git -C "$repo_root" worktree remove --force "$worktree" >/dev/null

"$target_dir/debug/cdp-transport-gate" attest \
	--repo-root "$repo_root" \
	--expected-git-revision "$revision" \
	--output "$output_dir/attestation.json"
test -s "$output_dir/attestation.json"
printf 'cross-worktree cached-binary regression passed for %s\n' "$revision"
