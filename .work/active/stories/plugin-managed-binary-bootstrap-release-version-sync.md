---
id: plugin-managed-binary-bootstrap-release-version-sync
kind: story
stage: done
tags: [distribution, release]
parent: plugin-managed-binary-bootstrap
depends_on: [plugin-managed-binary-bootstrap-launcher-and-installer]
release_binding: 1.0.1
gate_origin: null
created: 2026-07-16
updated: 2026-07-15
---

# Derive plugin versions during product releases

Extend the Cargo-authoritative release helper to update both native plugin manifests, both first-party marketplace entries, and the launcher version marker in the same transaction as Cargo metadata. Validate current identity before mutation, restore every file on failure, and stage all derived metadata in the release commit.

## Acceptance evidence

- Prepare, dry-run, failure rollback, and release staging are covered in isolated fixtures.
- Every Krometrail version-bearing file equals the root Cargo version after a successful prepare.
- Non-Krometrail release-helper fixtures retain Cargo-only behavior.
- Static distribution tests reject drift before a release can tag or publish.

## Implementation notes

- Extended the existing Cargo-authoritative helper only when the root package is `krometrail`; generic fixture repositories retain Cargo-only behavior.
- Derived both native manifests, both first-party catalog versions, and `plugin/version` after validating every source began at the current Cargo version.
- Included all projections in failure rollback and release commit staging.
- Added isolated successful prepare and forced-gate-failure rollback fixtures; the complete distribution contract suite passes.
