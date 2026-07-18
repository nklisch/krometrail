---
id: epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence
kind: story
stage: implementing
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
