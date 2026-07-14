---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache
kind: feature
stage: drafting
tags: [visual, storage]
parent: epic-temporal-debugging-workflow
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Bounded Artifact Generation and Cache

## Brief

Deliver the Krometrail adapter from a `ResolvedRange` and retained encoded frames to the browser-agnostic `temporal-vision` crate. The adapter decodes source images, preserves ordered frame identities and session-relative timing, maps declared gaps and caller markers, splits incompatible visual epochs rather than silently stretching them, and invokes the existing storyboard, orientation, difference-map, region-filmstrip, and other supported source-derived generators.

Run decoding and visual work under independent concurrency, memory, source-frame, and output bounds so an investigation cannot block capture ingestion or grow with session duration. Persist exact encoded artifacts and their existing provenance manifests through one artifact-store/cache authority; cache identity derives from ordered source frames, artifact kind, transformation parameters, and algorithm version, and retained hits return the same traceable evidence without regeneration.

This feature owns adaptation, bounded generation, persistence, lookup, and cache invalidation with source retention. It does not resolve natural anchors, compose the agent-facing debug bundle, interpret visual change as a diagnosis, or add a Krometrail-specific manifest parallel to `temporal-vision::ArtifactManifest`.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: generation foundation — consumes resolved ranges and is shared by the primary bundle and progressive region/artifact retrieval

## Simplification opportunity

- Turn the store's existing artifact schema and retention hooks into one production artifact port, replacing test-only direct rows and the root's no-op temporal-vision import. Reuse `FrameSource`, temporal-vision artifact/provenance types, and the authoritative usage ledger instead of adding another frame reader, manifest, cache index, or image pipeline.

## Foundation references

- `docs/ARCHITECTURE.md` — Artifact Generation, Temporal Visual Crate, Recording Store, and Failure Isolation
- `docs/SPEC.md` — Temporal Queries and Artifact Provenance
- `docs/VISUAL-EVIDENCE.md` — Shared Artifact Contract, Input Sequence, Determinism, and Provenance
- `docs/EVALUATION.md` — Performance Evaluation and Storage and Retention Evaluation
