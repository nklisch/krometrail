---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-contracts-and-region-semantics
kind: story
stage: done
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

## Implementation notes

- Execution capability: highest, selected by the caller because these contracts define future public evidence, geometry, provenance, and retention boundaries; direct-read only with no nested agent.
- Review weight: standard from the caller; review is not applicable at this child-story checkpoint and remains feature-scoped.
- Files changed: `.work/active/stories/epic-temporal-debugging-workflow-progressive-evidence-and-pinning-contracts-and-region-semantics.md`; `crates/krometrail-core/src/{progressive.rs,error.rs,lib.rs}`; `crates/krometrail-core/src/ports/{browser.rs,mod.rs,progressive.rs}`; `crates/temporal-vision/src/{geometry.rs,filmstrip.rs}`; `crates/temporal-vision/tests/filmstrip.rs`.
- Tests added: exhaustive eight-operation registry/wire pairing; malformed scope/selection/limit/resolved-range/region/mask coverage; source-handle privacy and payload-integrity coverage; exact single-epoch and locator validation; pin availability/union/overlap/idempotence invariants; fractional outward rounding and overflow; canonical rational viewport mapping; mask bounds, dimensions, application, legend, manifest, parameter identity, deterministic pixels, and no-mask golden preservation.
- Simplification: the progressive store is a zero-method intersection of the three existing ports; generic artifact generation is wrapped only to revalidate its resolved range and does not copy generator variants; viewport/source scaling and mask crop/application stay entirely in temporal-vision.
- Decisions: progressive direct-read resources use `ArtifactEvidenceHandle` and `SourceFrameHandle`, while request-scoped byte containers intentionally have no Serde implementation. Rich `progressive::PinChange`/`PinState` coexist with the currently implemented recording-store `PinChange` until the next coherent-store checkpoint migrates the existing retention methods and adapters.
- Discrepancies from design: existing generic-generation `ArtifactHandle`, `StoredArtifact`, and `RetentionStore` signatures were not changed in this checkpoint because doing so would require forbidden root/store/CDP writes. The progressive boundary nevertheless carries exact scoped artifact/source handles and rich pin reports; the already-designed consuming checkpoints perform the adapter migration without a compatibility facade or duplicate method set.
- Adjacent issues parked: none.

## Verification evidence

- `rustup run 1.85.0 cargo fmt --all -- --check` — passed.
- `rustup run 1.85.0 cargo check -p krometrail-core -p temporal-vision --all-targets --locked` — passed.
- `rustup run 1.85.0 cargo test -p krometrail-core -p temporal-vision --all-targets --locked` — passed, 146 tests across eight suites.
- `rustup run 1.85.0 cargo clippy -p krometrail-core -p temporal-vision --all-targets --locked -- -D warnings` — passed.
- `rustup run 1.85.0 cargo check --workspace --all-targets --locked` — passed as an additional reverse-dependency check.
- Existing no-mask filmstrip output remains byte-identical under its exact SHA-256 golden; masked output changes deterministically with mask identity and records the full validated mask in provenance.
