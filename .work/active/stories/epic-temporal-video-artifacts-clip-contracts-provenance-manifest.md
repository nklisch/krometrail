---
id: epic-temporal-video-artifacts-clip-contracts-provenance-manifest
kind: story
stage: done
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

## Implementation notes

- Execution capability: GPT-5.6 Sol at xhigh, selected by the active autopilot caller because this checkpoint defines persisted stable provenance and cache identity.
- Review weight: `standard` (autopilot default); this child checkpoint closes on green verification and the feature receives independent review.
- Files changed: `crates/krometrail-core/src/video/manifest.rs`, video module/lib exports, and the shared core video contract tests.
- Tests added: manifest round-trip and fixed silent MP4/H.264/yuv420p semantics; scope/media/audio/length contradiction rejection; canonical transcript equality; sensitivity to timing, source selection, gaps, geometry, output ceiling, encoder build identity, adapter/argument policy; and serialized privacy exclusions.
- Simplification: the manifest embeds the exact `VideoPresentationPlan` and `VideoEncodingProfile`; the versioned cache transcript serializes those same authorities plus fixed media values and server ceilings, with source fingerprints intentionally left to the retained-generation cache key.
- Discrepancies from design: none. Manifest construction additionally verifies that epoch frame IDs form one contiguous slice of the resolved scope and that each visible gap slate range is fully covered by its named scope gaps.
- Verification: `cargo fmt --all`, `cargo test -p krometrail-core --all-targets` (130 passed), focused cache sensitivity, and `cargo clippy -p krometrail-core --all-targets -- -D warnings` passed.
- Adjacent issues parked: none.
