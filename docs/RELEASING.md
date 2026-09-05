# Releasing

## Krometrail (the product)

One version authority: the root `Cargo.toml` `[package].version` (mirrored by
`[workspace.package].version`). All workspace members except temporal-vision
inherit it.

```bash
bun scripts/bump-version.ts patch   # or minor|major|x.y.z, --dry-run first
```

**Version ownership is not workspace membership.** A product bump moves the
root package and every member that explicitly inherits
`[workspace.package].version` (`version.workspace = true`, dotted or
inline-table form); any other member keeps its manifest and `Cargo.lock`
entry byte-identical — including crates with a version literal of either
quote style and crates with no version, which Cargo defaults to 0.0.0. The
lock verifier rejects any product refresh that touches such an entry.

Every shipped version projection is registered in one inventory,
`scripts/release-ownership.ts` — the plugin manifests for Claude Code, Codex,
and Antigravity, both marketplace catalogs, and the `plugin/version` launcher
marker. The helper rewrites exactly that inventory, refuses any registered
projection that does not carry the current product version, and refuses any
version-bearing file in the shipped surface (`plugin/`, `.claude-plugin/`,
`.agents/plugins/`) that the inventory does not list, so a new projection can
never be silently skipped by a release.

The script then commits, tags, and pushes. The `release.yml` workflow builds
and publishes platform artifacts from the tag.

## temporal-vision (the library)

**temporal-vision is versioned and published independently of the workspace.**
Its version lives as a literal in `crates/temporal-vision/Cargo.toml` and
`bump-version.ts` deliberately never touches it: it skips the crate's manifest
and lock entry, and it hard-fails if the crate is recoupled to the workspace
version in any TOML shape (`version.workspace = true`, dotted or inline
table).

Policy — bump exactly when the crate changes, never otherwise:

- **Missed-bump gate (CI, PRs):** a PR that changes anything under
  `crates/temporal-vision/` must bump its version in the same PR. This keeps
  every released change tied to one reviewed version number.
- **No-op release gate (publish workflow):** tagging a release with no
  source changes since the previous `temporal-vision-v*` tag fails — a new
  number over unchanged code is never published.

Release flow:

```bash
# 1. In the PR that changes the crate: bump version in
#    crates/temporal-vision/Cargo.toml (0.x line — independent semver).
# 2. Merge to main, then tag the merge commit:
git tag temporal-vision-vX.Y.Z
git push origin temporal-vision-vX.Y.Z
```

The `publish-temporal-vision.yml` workflow verifies tag == manifest version,
runs the no-op gate, tests the crate, and publishes to crates.io via
**Trusted Publishing** (GitHub OIDC → short-lived token; no stored secrets).
One-time configuration lives at crates.io → temporal-vision → Settings →
Trusted Publishing (repo `nklisch/krometrail`, workflow
`publish-temporal-vision.yml`, environment `crates-io`).
