---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-progressive-service-and-composition
kind: story
stage: done
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
depends_on:
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning-current-reference-geometry
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Compose the Progressive Evidence Service

## Checkpoint

Implement one `ProgressiveEvidenceService` over the composite existing-store port and existing `ArtifactGeneration`. Dispatch all eight registry operations, enforce source count/byte limits and deterministic ordering, resolve every region form to one fixed compatible visual epoch, and delegate generic/region generation to the implemented cache/single-flight service.

Root retains one concrete `RecordingStore`, supplies it to artifact generation and progressive evidence, and publishes `Arc<dyn ProgressiveEvidence>` in runtime dependencies. MCP remains unchanged; no tool, resource, schema copy, URI, raw path, payload map, debug bundle, or browser-event context is added.

## Files

- `src/progressive/{mod.rs,service.rs,region.rs}` (new)
- `src/artifacts/generators.rs` (fixed mask delegation only)
- `src/{main.rs,app.rs}`
- focused root/service tests

## Acceptance evidence

- Retrieve/list/fetch/generic-generate/region-generate/pin/unpin/query dispatch through one registry-associated service and preserve stable errors/recovery.
- All-frame lists use resolved capture order; explicit ID reads use request order and report both positions. MIME/format/geometry/times/ordinal/hash/length are exact.
- Runtime and caller count/per-item/total byte limits reject before partial return; payloads are request-scoped bytes, never base64/data URLs/paths.
- Source-pixel, outward-rounded viewport CSS, selected-frame rect/mask, and current-reference regions use one chosen locator frame and reject wrong scope, stale references, contradictory mapping, or multiple epochs.
- Region generation creates one existing `RegionFilmstripRequest`; all four generic variants and cache/single-flight/decode/publication paths remain owned by `ArtifactGeneration`.
- Root shares one store/generator/service authority and delays its only app-composition overlap until this checkpoint. `build_service` and the MCP crate remain untouched.

## Ordering

Depends on coherent store semantics and current-reference geometry. Qualification consumes the fully composed path.

## Outcome

Implemented one object-safe progressive evidence service in `src/progressive/` and composed it at the root as `Arc<dyn ProgressiveEvidence>`. The service dispatches the complete eight-operation registry over the existing shared `RecordingStore` and temporal-vision artifact service. It revalidates direct in-process source requests and store results, preserving exact scope, request/resolved positions, capture ordering, metadata, MIME type, hashes, payload lengths, and whole-result byte limits without adding a reader, cache, pin ledger, or payload table.

All region forms now collapse to one fixed temporal-vision request over one compatible visual epoch. CSS and live-reference rectangles round outward without clipping; selected masks retain their exact full-frame bitset and fixed bounds through artifact generation and provenance. Current-reference geometry is sampled exactly once before retained metadata is read, then mapped through the chosen source frame's recorded viewport. Wrong scope, missing live context, stale or contradictory geometry, absent source identity, and multi-epoch input fail before artifact generation.

Root composition retains one concrete `Arc<RecordingStore>` and projects that same allocation into progressive source, artifact, and retention ports. Generic and region artifact requests delegate to the existing artifact-generation service, preserving its decode, cache, single-flight, corruption-regeneration, cancellation, and bounded-work behavior. No MCP or browser-operation registration was added.

## Verification

Rust 1.85 verification passed:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

Focused tests cover all eight dispatches, all-frame and explicit-ID ordering, exact metadata and payload bytes, count/per-item/total limits, every region form, CSS outward rounding, exact selected masks, one-sample current geometry, missing/stale/contradictory scope, multi-epoch rejection, delegation to the four existing generator families, cache/provenance mask preservation, and root shared-store pointer identity.
