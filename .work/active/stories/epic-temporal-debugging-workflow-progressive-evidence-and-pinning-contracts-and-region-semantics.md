---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-contracts-and-region-semantics
kind: story
stage: implementing
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Define Progressive Evidence and Region Contracts

## Checkpoint

Establish the infrastructure-free progressive-evidence registry, typed requests/results, scoped artifact/source handles, request-scoped payload values, operation limits, current-reference geometry port, composite existing-store port, and richer pin state described in the parent feature. The registry is the sole eight-operation list and the future generated-schema authority.

Extend temporal-vision's existing fixed-region contract rather than copying its math: outward fractional viewport bounds, canonical frame viewport/image mapping, full-frame `BinaryMask::bounds`, and an optional visibly applied fixed filmstrip mask. Current reference and source-frame selection resolve to fixed geometry only; no type implies tracking or historical node identity.

## Files

- `crates/krometrail-core/src/progressive.rs` (new)
- `crates/krometrail-core/src/ports/progressive.rs` (new)
- `crates/krometrail-core/src/ports/{browser.rs,artifacts.rs,retention.rs,mod.rs}`
- `crates/krometrail-core/src/{artifacts.rs,error.rs,lib.rs}`
- `crates/krometrail-core/src/recording/retention.rs`
- `crates/temporal-vision/src/{geometry.rs,filmstrip.rs,lib.rs}`
- `crates/temporal-vision/tests/filmstrip.rs`

## Acceptance evidence

- One exhaustive registry associates retrieve/list/fetch/generic-generate/region-generate/pin/unpin/pin-state requests and results; validated Serde rejects malformed scope, selection, region, mask, and bounds.
- Handles carry typed ID, session/target scope, MIME, SHA-256, length, and exact provenance without paths, base64/data URLs, MCP identifiers, or Serde payload bytes.
- `ProgressiveEvidenceStore` only intersects `FrameSource + ArtifactStore + RetentionStore`; it declares no duplicate read/cache/pin methods.
- Source-pixel, viewport-CSS, caller-selected frame rect/mask, and current-reference declarations are fixed and explicit. Source-frame selection is caller-supplied, never CV or tracking.
- Temporal-vision owns both outward mapping stages, mask bounds/application, padding, visible mask legend, manifest mask, deterministic parameters, and cache-affecting request data.
- `PinState` reports exact activation, complete/partial/unavailable expected frames, actual protected segment ranges/bytes, true coalesced unions, source-only scope, pinned usage, and final retention status without a schema migration.

## Ordering

This checkpoint has no sibling dependency. Coherent store and current-browser checkpoints consume these contracts. It is a design checkpoint for one feature owner, not an independent worker assignment.
