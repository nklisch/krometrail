---
id: epic-agent-browser-ergonomics-viewport-intent-contract
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-viewport-intent
depends_on: []
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Define viewport intent and preset materialization

Add the stable additive preset wire variant, exact vendor-neutral preset table, intent/provenance result, independently observed layout geometry, and pure mismatch-guidance classification while preserving custom and clear encodings.

## Acceptance evidence

- Core tests lock existing JSON compatibility and all five preset metrics/intents.
- Guidance boundary tests cover general mismatch and likely missing viewport metadata without page-content disclosure.
- Custom and clear retain their existing meaning and exact request encoding.

## Ordering

This contract checkpoint has no sibling dependency. Runtime lifecycle integration depends on it.

## Implementation notes

- Execution capability: direct inline implementation of the cohesive core-domain and stable-wire contract.
- Review weight: standard child story; no independent story review required before the parent feature review.
- Changed `crates/krometrail-core/src/browser/{viewport.rs,mod.rs,operation.rs}` and `crates/krometrail-core/src/lib.rs`.
- Added five vendor-neutral presets with exact metrics, responsive-CSS versus mobile-device intent, pure materialization into the existing metrics authority, explicit no-user-agent provenance, independently modeled layout geometry and viewport-metadata presence, and bounded mismatch guidance.
- Preserved the exact stable custom and clear JSON encodings while adding the strict preset variant. Mixed and unknown variant fields fail closed through a validated wire decoder.
- Guidance uses the designed strict `max(8 CSS px, 5%)` threshold and emits at most one content-free message, with a specific missing-viewport-metadata classification only for mobile intent, absent metadata, and the 1.5× layout-width condition.
- Simplification: presets materialize immediately to `ViewportMetrics`; no device catalog, persisted preset state, or parallel viewport authority was introduced.
- Discrepancies and adjacent findings: none.

## Verification

- `cargo fmt --all`
- `cargo test -p krometrail-core browser::viewport::tests --locked`
