---
id: epic-temporal-vision-toolkit-motion-history-rendering
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-motion-history
depends_on: [epic-temporal-vision-toolkit-motion-history-decay-and-plan]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Motion-History Composition, Deterministic PNG, and Provenance

## Scope

Implement `generate_motion_history` in `crates/temporal-vision/src/motion_history.rs`,
composing the plan from `epic-temporal-vision-toolkit-motion-history-decay-and-plan`
into one checked RGB8 canvas, encoding it through the shared render seam, and assembling
the `ArtifactManifest`.

## Contracts to consume unchanged

- The shared render seam established by the sibling artifact features: `render.rs` (with
  `ArtifactLabels`, `RenderLimits`), `render/font.rs` (checked-in bitmap font, escaping,
  ellipsizing), `encode.rs` (deterministic bounded PNG + SHA-256), and the encoded-artifact
  result types `EncodedImage` / `GeneratedArtifact`. If motion-history lands first, introduce
  the seam in that canonical layout (no motion-history-specific behavior in the shared files).
- `pub(crate) linear_luminance([u16; 3]) -> u16` from `measure.rs` (integer Rec.709-weighted
  luminance; exposed by the sibling accumulation features or by motion-history if first).
- `ArtifactManifest::from_sequence`, `ArtifactKind::MotionHistory`, `EvidenceClass::SourceDerived`,
  `AlgorithmDescriptor`, `NormalizationStep`, `OutputHash`, `Parameters`, `ParameterValue`.
- `MotionHistoryPlan`, `MotionDecay`, `MotionHistoryParameters`, `build_motion_history_plan`
  from the parent story.

## Implementation notes

- Layout from `plan.dimensions()` plus fixed integer header/footer annotation heights; reject
  if width/height or canvas bytes exceed `parameters.limits`. No decorative border around
  source-derived pixels.
- Header band: caller `labels.title` and `labels.source`, escaped and ellipsized into fixed rows.
- Main area, per pixel: `lum = linear_luminance(reference_rgb16)`; subdued backdrop
  `gray = (lum * reference_strength + 32_767) / 65_535`; alpha `= accumulation[p]`; straight-alpha
  composite `out_c = (gray * (65_535 − alpha) + accent_c * alpha + 32_767) / 65_535`.
- Outline overlay drawn last: each `plan.outline()` pixel overwritten with `outline_color`.
- Footer band: decay legend (accent brightness ramp, ranks 0..=min(live_window−1,
  max_segment_rank), labeled `NEWEST`/`OLDEST RETAINED`); start/end timestamps and total span;
  `GAP — N declared; unseen behavior may have occurred` when `source.gaps()` is nonempty;
  `MOTION HISTORY — source-derived; no direction inferred`; explicit `TIME →`.
- Encode RGB8 PNG via the shared seam (pinned filter/compression, no ancillary chunks), cap
  encoded bytes; `OutputHash` = SHA-256 of exact returned bytes.
- Manifest via `from_sequence` with `MotionHistory`, `SourceDerived`,
  `AlgorithmDescriptor("motion-history", "1.0.0")`, `selected_frame_ids = [reference]`,
  `normalization = normalized.normalization_steps() + [measurement.provenance_step()]`,
  parameters recording all motion-specific choices and the PNG encoder profile.

## Acceptance evidence

- One combined image composes subdued luminance reference + accent-tinted motion intensity +
  white outline; no separate layer outputs; no `Layer` abstraction.
- All annotations visible, derive from manifest values, never alter source pixels with borders.
- Identical input → identical plan, canvas, PNG bytes, SHA-256, parameters, manifest across runs.
- Checked layout rejects excess width/height/canvas/encoded bytes; memory independent of input
  text length and session duration.
- `selected_frame_ids == [reference]`, `omitted_frame_count == source_frame_count − 1`,
  normalization records canonical steps plus threshold.
- No direction arrow, velocity vector, trajectory, optical-flow field, or inferred claim;
  `evidence_class == SourceDerived`.
- Shared bitmap font + escaped text require no host font, locale, shaping engine, UI toolkit,
  filesystem, browser, or GPU.

## Out of scope

Integration tests — those belong to
`epic-temporal-vision-toolkit-motion-history-public-contract-tests`. This story may add only
focused colocated private-mechanics tests for composition arithmetic if needed.

## Implementation evidence

- Added `generate_motion_history` and `MotionHistoryArtifact`, reusing the shared RGB8 canvas, bitmap-font, deterministic PNG, SHA-256, measurement classifier, and linear-luminance seams.
- The fixed composition draws one subdued reference plus accent intensity plus a final 4-connected outline; it emits no independently selectable layers and introduces no rendering framework.
- Header/footer annotations derive from the same range, decay, gap, and parameter values recorded in the manifest. The footer explicitly states source-derived evidence, no direction inference, chronological time, and storyboard/region-filmstrip disambiguation.
- Manifest provenance records the reference frame, complete source set, canonical normalization and threshold steps, integer decay/accumulation choices, segment/gap statistics, colors, bounds, fixed layout, and pinned encoder profile.
- Combined accumulator/mask/canvas working memory, output dimensions, canvas bytes, and encoded bytes are checked before partial artifact success.
- Verification: `cargo test -p temporal-vision --locked` (45 passed) and `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` passed.
- Execution capability: raised/high; review weight: standard (autopilot caller).
- Discrepancies from design: added an explicit display conversion provenance step after the required canonical normalization and threshold steps, matching the established difference-map renderer and making the RGB8 composition reproducible.
- Blockers and adjacent issues: none.
