---
id: epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence
kind: story
stage: done
tags: [visual, storage, security]
parent: epic-temporal-video-artifacts-retained-generation
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Additive temporal-video artifact persistence

## Design checkpoint

Add the video-specific retained publication/read contracts and generalize the private artifact engine, schema, files, recovery, usage, retention, and deletion path to accept validated `video/mp4` rows without changing existing image types or serialized manifests. Schema v6 must transactionally preserve every v5 image row and source link; there remains one artifact table, directory, cache lock authority, usage class, recovery pass, and deletion journal.

## Acceptance evidence

- Core contract tests prove video publication/read/manifest validation and byte-identical existing image serialization.
- Migration tests create retained v5 image state, apply v6, compare its row/source fields exactly, reopen the store, and read the original PNG.
- Real-store tests prove MP4 publish/lookup/scoped-read, equal-key concurrency, corruption invalidation, staged/crash recovery, source/budget eviction, pins, cancellation, and session deletion through the same authorities used by images.
- No FFmpeg or browser is required; this checkpoint validates typed retained bytes and provenance, not the production codec.

## Ordering constraints

- Root checkpoint for this feature.
- The generation service depends on these exact `ArtifactStore` video methods and must not add a video store, path grammar, index, or cleanup path.

## Execution contract

- Worker capability: highest available, selected by active autopilot because stable retained data and migration/deletion correctness are high consequence.
- Review weight: `standard` from autopilot default; this child closes on green evidence and the integrated feature receives the single independent review pass.

## Implementation notes

- Implemented by the highest-available Sol worker at `xhigh` reasoning; the feature retains the orchestrator's `standard` review weight.
- Added constructor-validated temporal-video generation, retained publication/read, selector-provenance, and manifest/cache contracts in `krometrail-core` without changing the existing image wire shapes.
- Migrated the shared artifact registry to schema v6 with exact image/video kind and media constraints. Video MP4 bytes now use the same artifact table, source links, cache-lock registry, directory, recovery scan, usage accounting, retention eviction, and deletion journal as PNG artifacts.
- Generalized the private stored-artifact validation/read path while preserving the public image types; private enum payloads are boxed so either artifact class does not inflate reads of the other class.
- Added migration evidence that v5 artifact and source-link fields survive v6 exactly, plus real-store video publish/cache/scoped-read/reopen/deletion and corruption-invalidation evidence. The existing shared-engine tests continue to cover cancellation, staged recovery, pins, budget/source eviction, and session deletion.
- Verification: `cargo check -p krometrail-core --all-targets --locked`; `cargo check -p krometrail-store --all-targets --locked`; `cargo test -p krometrail-core --locked` (137 passed); `cargo test -p krometrail-store --test video_artifact_store --locked` (2 passed); focused v5-to-v6 migration test; `cargo clippy -p krometrail-core -p krometrail-store --all-targets --locked -- -D warnings`; `git diff --check`.
- Simplification pass: retained one persistence engine and added only an exact artifact-kind discriminator; no parallel video index, path grammar, cache authority, recovery pass, or cleanup path was introduced.
- Discrepancies: none. Parked work: none.
