---
id: epic-temporal-vision-toolkit-frame-sequence-contracts-provenance-manifest
kind: story
stage: implementing
tags: [visual]
parent: epic-temporal-vision-toolkit-frame-sequence-contracts
depends_on: [epic-temporal-vision-toolkit-frame-sequence-contracts-sequence-and-annotations]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Project Deterministic Artifact Provenance

## Checkpoint

Implement `crates/temporal-vision/src/provenance.rs` with the stable artifact, evidence, and normalization registries; deterministic algorithm parameters; validated SHA-256 `OutputHash`; ordered normalization steps; and generic `ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId>`.

The one registry declarations publish these stable wire sets:

- artifacts: `before_during_after`, `storyboard`, `difference_map`, `region_filmstrip`, `motion_history`;
- evidence: `source_frame`, `source_derived`, `inferred`;
- normalization: `color_space_conversion`, `alpha_compositing`, `integer_scaling`, `fixed_crop`, `denoising`, `thresholding`.

`Parameters` uses a lexicographically ordered `BTreeMap` and tagged recursive `ParameterValue` values. Reject empty keys recursively and non-finite numbers; canonicalize negative zero. Preserve list and normalization-step order. `OutputHash` is the SHA-256 of exact returned encoded artifact bytes and serializes as exactly 64 lowercase hex characters.

`ArtifactManifest::from_sequence` is the normal constructor. It derives ordered source IDs, inclusive range, marker/gap records, region, complete bit mask, and source/omitted counts from the authoritative sequence. The caller supplies only artifact identity/kind/class, algorithm descriptor, selected frame IDs, normalization, artifact parameters, output dimensions, and output hash. Selected IDs must be a unique ordered subsequence. Custom generic deserialization invokes the same validation so persisted counts, ordering, geometry, and hashes cannot bypass the contract.

## Acceptance evidence

- Each growing registry derives `ALL`, `as_str`, display, validation, and serde behavior from one declaration with exhaustive table-driven coverage.
- Nested parameters serialize deterministically and reject NaN, infinity, negative-zero ambiguity, and empty keys.
- Output hashes round-trip as lowercase SHA-256 hex and reject uppercase, wrong length, and non-hex forms.
- A manifest cannot disagree with its sequence about source order/count, selected order/count, range, annotations, geometry, region, or mask.
- The complete mask remains machine-readable for reproduction; source pixel bytes do not enter the manifest.
- Generic string/newtype IDs round-trip without importing Krometrail identity types.

## Ordering

Depends on `epic-temporal-vision-toolkit-frame-sequence-contracts-sequence-and-annotations`, whose aggregate is the provenance source of truth.
