---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-release-provenance
kind: story
stage: implementing
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

- [ ] A branch named like a release cannot supply release artifacts or create a tag implicitly.
- [ ] Every artifact and the published release are bound to one verified tag SHA.
- [ ] Distribution tests leave repository release outputs byte-for-byte unchanged.
- [ ] Lock refresh validation cannot hide duplicate-name dependency changes.
- [ ] Rust and distribution gates pass.

## Review origin

Filed from the second GPT-5.6 Sol adversarial feature review.
