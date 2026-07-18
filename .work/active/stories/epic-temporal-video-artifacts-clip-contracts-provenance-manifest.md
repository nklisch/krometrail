---
id: epic-temporal-video-artifacts-clip-contracts-provenance-manifest
kind: story
stage: implementing
tags: [visual, storage, security]
parent: epic-temporal-video-artifacts-clip-contracts
depends_on: [epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port, epic-temporal-video-artifacts-clip-contracts-presentation-planner]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Typed temporal-video provenance manifest

## Design checkpoint

Add the constructor-validated video manifest and canonical cache-parameter transcript that embed the exact presentation plan alongside scope, output geometry/profile, bounded encoder identity, encoded length, and output hash. This is a video-specific core contract; it does not replace the existing still-image manifest or implement the later persisted artifact envelope.

## Acceptance evidence

- Round-trip tests reject any manifest contradiction and preserve exact segment timing, source/gap mapping, holds, media/no-audio profile, encoder identity, output length, and hash.
- Canonical transcript sensitivity covers every plan, geometry, ceiling, encoder-build/name, adapter, and argument-policy field while remaining stable for identical input.
- Serialized output contains no executable/temp path, raw build report, raw stderr, source image bytes, or provider-specific value.

## Ordering constraints

- Depends on both the domain/port and deterministic planner checkpoints.
- Later storage/cache work consumes this type and adds source fingerprints; it must not introduce a second timing or encoder-identity transcript.
