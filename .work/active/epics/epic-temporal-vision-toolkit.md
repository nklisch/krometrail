---
id: epic-temporal-vision-toolkit
kind: epic
stage: review
tags: [visual]
parent: null
depends_on: [epic-rust-cdp-capture-foundation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-13
---

# Temporal Vision Toolkit

## Brief

This epic delivers the browser-agnostic Rust crate that turns timestamped image sequences into compact temporal visual evidence. The crate owns generic frame and marker inputs, direct visual-change measurements, deterministic representative-frame selection, artifact rendering, and reproducible provenance.

The toolkit remains independent of Chrome, CDP, Krometrail storage, MCP, DOM state, and framework types. Its outputs distinguish source frames, source-derived transformations, and inferred analysis so callers can trace every visual claim back to authoritative evidence.

This epic does not own browser capture, persistent sessions, agent tool schemas, logical element tracking, or automatic diagnosis. Those responsibilities remain in Krometrail or in separately labeled inferred-analysis extensions.

## Foundation references

- `docs/VISION.md` — Visual Evidence and Reusable Temporal Vision
- `docs/ARCHITECTURE.md` — Temporal Visual Crate and Artifact Generation
- `docs/VISUAL-EVIDENCE.md` — complete artifact and provenance contract
- `docs/EVALUATION.md` — Artifact Evaluation Conditions and Artifact-Specific Evaluation

## Design decisions

- **Processing model:** Expose an immutable batch-sequence API that generates measurements and artifacts on demand. Streaming and rolling analysis remain caller responsibilities until measured workloads demonstrate that a stateful crate API is necessary.
- **Browser-agnostic proof:** Validate reuse through deterministic synthetic and non-browser frame-sequence examples inside the crate. A real Silas integration is not part of this epic and can consume the crate later without shaping its initial public surface.

## Design decisions

- **Processing model:** Expose an immutable batch-sequence API that generates measurements and artifacts on demand. Streaming and rolling analysis remain caller responsibilities until measured workloads demonstrate that a stateful crate API is necessary.
- **Browser-agnostic proof:** Validate reuse through deterministic synthetic and non-browser frame-sequence examples inside the crate. A real Silas integration is not part of this epic and can consume the crate later without shaping its initial public surface.
- **Crate independence:** `temporal-vision` does not depend on `krometrail-core`, Krometrail storage, CDP, MCP, or DOM types. It accepts generic frame identifiers and timestamps. Krometrail-specific mapping happens in the adapter built by `epic-temporal-debugging-workflow`.
- **Single working pixel format:** Start with one common decoded representation (RGB[A]8) and keep normalization parameters explicit in provenance. Avoid image-decoder and color-management generality until evaluation shows it is needed.
- **Source-versus-inference boundary:** All artifacts in this epic are source-derived. Inferred overlays (tracking, optical flow, direction) require a separate future extension with their own method/version/confidence contract.

## Decomposition

The epic is split into six capability-shaped features. The first two are foundational: they define the input/provenance vocabulary and the normalized pixel/measurement pipeline that every artifact shares. The remaining four are independent artifact renderers that can proceed in parallel once measurements land, because they share only the prepared pixels and metrics.

### Child features

- `epic-temporal-vision-toolkit-frame-sequence-contracts` — generic frame, sequence, marker, gap, region, mask, and provenance contracts — depends on: `[]`
- `epic-temporal-vision-toolkit-normalization-and-measurements` — normalization to a common pixel representation and deterministic visual-change measurements — depends on: `[epic-temporal-vision-toolkit-frame-sequence-contracts]`
- `epic-temporal-vision-toolkit-storyboard` — representative-frame selection, temporal storyboard, and before/during/after orientation composite — depends on: `[epic-temporal-vision-toolkit-normalization-and-measurements]`
- `epic-temporal-vision-toolkit-difference-map` — reference, change-frequency, and change-timing panels — depends on: `[epic-temporal-vision-toolkit-normalization-and-measurements]`
- `epic-temporal-vision-toolkit-region-filmstrip` — fixed-region crops, locator image, and explicit tracking-method contract — depends on: `[epic-temporal-vision-toolkit-normalization-and-measurements]`
- `epic-temporal-vision-toolkit-motion-history` — source-derived motion-history image with decay legend and changed-region outlines — depends on: `[epic-temporal-vision-toolkit-normalization-and-measurements]`

### Decomposition risks

- **Single working pixel format:** Early choice of RGBA8 may need expansion for higher bit depth or grayscale evaluation inputs. The contract is intentionally generic about pixel format so the internal representation can evolve without changing sequence/provenance types.
- **Selection algorithm coupling:** Storyboard, difference map, region filmstrip, and motion history all rely on the same thresholded change measurements. If the metric proves insufficient for any artifact, changing it affects all four; the crate must version the measurement algorithm in provenance.
- **No browser fixtures yet:** The crate will be validated with deterministic synthetic sequences. Cross-browser evaluation waits for `epic-prove-temporal-advantage` and `epic-temporal-debugging-workflow`, so the design must keep adapters outside the crate.
- **Motion-history value unproven:** `EVALUATION.md` warns that an artifact that consistently harms interpretation should be removed from the default bundle. This feature is explicitly framed as a bounded experiment; the default debug bundle may omit it.

## Child features reviewed and complete

All six capability features reached `done` with green package/integration verification and
feature-level review:

- `epic-temporal-vision-toolkit-frame-sequence-contracts`
- `epic-temporal-vision-toolkit-normalization-and-measurements`
- `epic-temporal-vision-toolkit-storyboard`
- `epic-temporal-vision-toolkit-difference-map`
- `epic-temporal-vision-toolkit-region-filmstrip`
- `epic-temporal-vision-toolkit-motion-history`

The crate now provides generic validated frame/provenance contracts, deterministic normalization
and direct measurements, and four source-derived artifact families over one shared bounded renderer
and encoder seam. Motion history remains an opt-in bounded experiment pending evaluation rather
than a default-bundle claim. The epic is ready for deeper aggregate review of cross-artifact
contracts, deterministic evidence semantics, and browser-independent reuse.
