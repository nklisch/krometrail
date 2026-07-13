---
id: epic-temporal-vision-toolkit-frame-sequence-contracts
kind: feature
stage: drafting
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Frame, Sequence, and Provenance Contracts

## Brief

This feature delivers the crate's input and provenance contracts: a generic `Frame`, an ordered `FrameSequence`, caller-supplied markers, declared capture gaps, regions, masks, and the `ArtifactManifest` that every generated artifact returns.

The contracts are browser-agnostic and infrastructure-free. They do not depend on Chrome, CDP, Krometrail storage, MCP, DOM, or framework types. A frame carries only an identifier, timestamp, dimensions, pixel format, and pixel payload. A sequence carries frames in deterministic order plus optional markers, region, mask, and gap annotations. Provenance records the artifact kind, evidence class, algorithm version, source and selected frame identifiers, omitted-frame count, range, markers, gaps, region, normalization, parameters, output dimensions, and output hash.

This feature does not decode pixels, measure change, or render images. It supplies the typed vocabulary that normalization, measurement, and rendering features share.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: foundation feature — every other feature depends on its contracts

## Simplification opportunity

- Keep the initial pixel format small (e.g., RGBA8) and require callers to decode into that common representation. Avoid building a generic image-codec pipeline inside the crate; the crate operates on decoded pixels.
- Defer streaming or incremental sequence APIs. The first shape is an immutable batch sequence; callers with streaming needs can build that themselves until workloads prove otherwise.
- Treat inferred-analysis provenance as a distinct evidence-class label rather than a separate crate module.

## Foundation references

- `docs/VISION.md` — Reusable Temporal Vision
- `docs/ARCHITECTURE.md` — Temporal Visual Crate
- `docs/VISUAL-EVIDENCE.md` — Input Sequence, Shared Artifact Contract, Provenance, Determinism

<!-- The design pass on this feature will fill in interfaces, signatures, and implementation units. -->
