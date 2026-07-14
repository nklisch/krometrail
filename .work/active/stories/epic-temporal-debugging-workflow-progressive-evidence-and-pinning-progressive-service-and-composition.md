---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-progressive-service-and-composition
kind: story
stage: implementing
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
