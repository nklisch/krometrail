---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-contracts-and-cache-identity
kind: story
stage: done
tags: [visual, storage]
parent: epic-temporal-debugging-workflow-artifact-generation-and-cache
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Define Artifact Contracts and Cache Identity

## Checkpoint

Establish the infrastructure-free application request/result/service contract and focused artifact-store port described in the parent feature. Core consumes one existing `ResolvedRange`, exposes four generator request variants, and aliases the exact generic `temporal_vision::ArtifactManifest` with Krometrail IDs. Storyboard orientation remains the existing optional `BeforeDuringAfter` output, not a separately dispatched generator.

Centralize temporal-vision generator descriptors so generators and pre-generation cache identity use one name/version authority. Implement the versioned, length-prefixed SHA-256 cache transcript over ordered source identity/content/metadata, output kind, canonical effective parameters including markers/gaps, visual epoch, and generator/adapter versions.

## Files

- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/artifacts.rs` (new)
- `crates/krometrail-core/src/ports/artifacts.rs` (new)
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/{lib.rs,error.rs}`
- `crates/temporal-vision/src/{provenance.rs,lib.rs,render.rs,difference_map.rs,filmstrip.rs,motion_history.rs}`
- `src/artifacts/cache.rs` (new)

## Acceptance evidence

- Request constructors/Serde reject empty generators, duplicate/out-of-range markers, invalid scales/tiles/limits, and malformed variants.
- `ArtifactGeneration` and `ArtifactStore` are object-safe and expose no runtime, SQL, or filesystem types.
- The result/store boundary carries the exact temporal-vision manifest alias and artifact kind; no parallel manifest or kind registry exists.
- Cache tests prove each required input changes the key and that explicit defaults equal materialized defaults.
- All generators consume the one descriptor registry and existing temporal-vision output hashes remain unchanged.

## Ordering

This checkpoint has no sibling dependency. The schema/publication checkpoint consumes these port types and cache metadata. 

## Implementation notes

- Execution capability: highest; the caller selected it for the cross-cutting provenance, cache, and future publication boundary.
- Review weight: standard from the autopilot caller; child checkpoints do not receive independent review.
- Files changed: `crates/krometrail-core/{Cargo.toml,src/artifacts.rs,src/error.rs,src/lib.rs,src/ports/{mod.rs,artifacts.rs}}`, `crates/temporal-vision/src/{provenance.rs,lib.rs,render.rs,difference_map.rs,filmstrip.rs,motion_history.rs}`, root `Cargo.toml`, and `src/{main.rs,artifacts/{mod.rs,cache.rs}}`.
- Tests added: strict artifact-request construction/Serde validation, orientation registry semantics, one-field-at-a-time length-framed cache identity sensitivity, algorithm/adapter version sensitivity, and exact encoded-frame fingerprinting. Existing temporal-vision suites verify unchanged output bytes/hashes after descriptor centralization.
- Verification: `cargo fmt --all`; focused all-target tests for `temporal-vision`, `krometrail-core`, and root (139 passed); focused all-target Clippy with `-D warnings` (green).
- Simplification: the temporal-vision generator constants now derive from one descriptor registry; Krometrail aliases the exact manifest and defines one artifact cache/store port rather than parallel provenance or path-bearing storage values.
- Discrepancies from design: the authoritative `ArtifactCacheKey` lives in core's store port rather than being redefined in the root cache module; `ArtifactLookup::Hit` boxes the large stored value to keep the public enum compact. Cache code is test-compiled until the bounded-service checkpoint consumes it, avoiding dead production scaffolding.
- Adjacent issues parked: none.
