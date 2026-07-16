---
id: plugin-managed-binary-bootstrap-release-version-sync
kind: story
stage: implementing
tags: [distribution, release]
parent: plugin-managed-binary-bootstrap
depends_on: [plugin-managed-binary-bootstrap-launcher-and-installer]
release_binding: null
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
