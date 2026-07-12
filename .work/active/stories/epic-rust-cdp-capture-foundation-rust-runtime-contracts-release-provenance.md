---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-release-provenance
kind: story
stage: done
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Bind release artifacts to an immutable tag commit

## Scope

Close the second adversarial review's release-integrity findings. A release must resolve an existing tag exactly once, build every artifact from that immutable commit, and publish only to that same tag. Distribution tests must not mutate repository release outputs, and lockfile-delta validation must handle duplicate package names safely.

## Requirements

- Require `refs/tags/<v-semver>` to exist; reject ambiguous branch-like refs.
- Resolve and expose the tag commit SHA once, check out that exact SHA in every build job, and assert the publication tag resolves to it.
- Run the stale developer-install fixture entirely inside its temporary repository and prove the real repository `target/release/krometrail` is unchanged.
- Compare lockfile packages as a multiset keyed by name, version, source, and checksum, allowing only expected workspace-version changes.
- Add positive and negative static/isolated tests for all paths.

## Acceptance criteria

- [x] A branch named like a release cannot supply release artifacts or create a tag implicitly.
- [x] Every artifact and the published release are bound to one verified tag SHA.
- [x] Distribution tests leave repository release outputs byte-for-byte unchanged.
- [x] Lock refresh validation cannot hide duplicate-name dependency changes.
- [x] Rust and distribution gates pass.

## Review origin

Filed from the second GPT-5.6 Sol adversarial feature review.

## Implementation notes

- Files changed: `.github/workflows/release.yml`, `scripts/validate-release-tag.sh`, `scripts/verify-release-tag-identity.sh`, `scripts/bump-version.ts`, `tests/distribution-static.sh`.
- Tests added: exact lightweight and annotated tag fixtures; release-shaped branch rejection; remote tag identity mismatch checks; immutable checkout/publish wiring assertions; temporary-repository developer-install isolation with byte-for-byte repository-target protection; duplicate-name Cargo.lock multiset positive and mutation/rollback negative fixtures.
- Verification: Rust fmt/check/test/clippy with locked dependencies, installer and distribution shell syntax, distribution static contracts, release workflow YAML parsing, and `git diff --check` all passed.
- Design decisions: the validator reads `Cargo.toml` from the exact resolved tag commit and emits only its SHA; build and publication jobs consume that single job output. Publication identity is checked before and after upload through the exact remote tag ref. Lock comparisons preserve complete package records while using name/version/source/checksum identity fields and multiplicity, normalizing only source-less workspace package versions.
- Discrepancies from design: none.
- Dispatch: direct local reads and implementation only, per caller instruction; no questions or subagents. `.pi/` was not edited or staged.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The orchestrator independently reran all 40 workspace tests, locked clippy, and the full hermetic distribution suite, and spot-checked exact-tag SHA propagation through build and publication. Acceptance boxes were aligned with the green evidence. Verdict: Approve - story verified by implement; fast-lane advance.
