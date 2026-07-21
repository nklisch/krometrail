---
id: feature-analysis-provenance-truthfulness
kind: feature
stage: review
tags: [visual, agent-ux, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Truthful analysis-artifact provenance

## Brief

Absorbs `idea-analysis-sampling-provenance-accuracy` and adds a third defect
found in the seventh shakedown. All three are false or contradictory provenance
claims in a product whose entire value proposition is trustworthy temporal
evidence.

**1. Manifest counts contradict the tool warning.** For the shakedown's
difference map over 474 frames, `generate_artifacts` warned
"difference_map analysis sampled 93 of 474 source frames with uniform spacing",
while the same artifact's manifest reported `selected_frame_count: 1`,
`omitted_frame_count: 473`. Both are authoritative surfaces describing one
artifact and they disagree. The cause: `omitted_frame_count = source_frame_count
- selected_count` (`crates/temporal-vision/src/provenance.rs:614`), and for
analysis generators `selected_frame_ids` holds only the reference frame, not the
93 analyzed frames. An agent auditing evidence completeness from the manifest
concludes 473 of 474 frames contributed nothing, when in fact 93 were analyzed
and 381 were dropped.

**2. Undecimated analysis manifests falsely claim `mode: "uniform_bounded"`.**
`decode_plan` (`src/artifacts/epoch.rs:283-291`) always attaches source
provenance, so `source_indices()` is always `Some` in production. Consequently
`analysis_sampling_parameters` (`crates/temporal-vision/src/provenance.rs:268`)
emits an `analysis_sampling` block with hardcoded `mode: "uniform_bounded"` and
`spacing: "uniform"` on *every* difference map and motion history, including
exhaustive undecimated runs. The top-level warning is correctly suppressed when
counts are equal, so no agent is told its evidence is degraded when it is not;
the defect is narrower but still a false claim. The storyboard path already
guards emission on actual decimation
(`src/artifacts/generators.rs:175-179`) — the analysis path should either guard
the same way or record the *requested* mode.

**3. `uniform_bounded` plus an explicit frame reference can drop that frame.**
`plan_for_analysis_sampling` (`src/artifacts/service.rs:690`) passes `None` as
`bounded_plan`'s `include_frame_id`, though `bounded_plan` has a retention
mechanism for exactly this (the filmstrip locator uses it). With wire-default
sampling on a large range, a `FrameSelector::Frame(id)` reference that falls off
the uniform grid fails with "reference frame is outside this visual epoch"
(`src/artifacts/generators.rs:685`). The message is misleading — the frame *is*
in the epoch; sampling dropped it — and the failure is avoidable.

## Simplification opportunity

Defects 1 and 2 are the same missing distinction: the manifest does not separate
"frames analyzed" from "frames rendered/referenced". Introduce that distinction
once in the manifest rather than special-casing each generator, and let both the
counts and the sampling block derive from it. That should also let the tool
warning and the manifest read from a single source so they cannot disagree
again.

Fold in if cohesive:
- `idea-artifact-error-context` — thread session/target/frame identity through
  artifact decode, epoch, generation, source-loss, deletion, and corruption
  errors, without exposing encoded bytes, filesystem paths, or cache internals.

Nits to fold in:
- `analysis_effective_max_frames` (`src/artifacts/service.rs:695`) divides by the
  per-frame max with no defensive floor; safe only via the non-empty-plan
  invariant. A `.max(1)` on the divisor makes the panic structurally impossible.
- `scripts/check-wire-enum-schemas.sh` misses single-line enum bodies
  (`enum H { A, B }`). Unreachable in-tree because `cargo fmt --check` is
  enforced and rustfmt always breaks enum bodies, but a real gap in the guard.

## Architectural choice

The manifest gains a third frame population. Previously it named only
`source_frame_ids` (everything retained in the epoch) and `selected_frame_ids`
(what the output renders or references), and derived `omitted_frame_count` from
the gap between them. That conflates two unrelated questions — "what evidence
was examined?" and "what does the picture show?" — and it is why an analysis
artifact that examined 93 frames and rendered a reference to 1 reported 473
omissions.

`analyzed_frame_ids` now names the frames that actually contributed. It is
derived once, inside `ArtifactManifest::from_sequence_with_trace_and_domain`.
Every count follows from it:

- `omitted_frame_count = source - analyzed` — frames that contributed nothing.
- analyzed-but-unrendered = `analyzed - selected`, derivable by any reader.

**Decoding a frame is not consuming it.** *Revised after cross-model review.*
Deriving the analyzed population from `sequence.frames()` alone was right for
difference map, motion history, and storyboards — all of which read every frame
they are handed — and wrong for the region filmstrip, whose tiles are chosen by
position. With five source frames and a three-tile limit, frames 1 and 3 were
neither rendered nor referenced anywhere, yet the manifest reported
`analyzed = 5, omitted = 0` while the filmstrip's *own* `omitted_frame_count`
parameter reported 2. The manifest contradicted its own parameter block.

The fix keeps the derivation central and adds exactly one declaration per
generator: `SequenceConsumption::{EveryDecodedFrame, SelectedFramesOnly}`. A
generator states which *shape* it has; the manifest derives every population from
that. A generator therefore cannot get the counts wrong, only the shape — and the
shape is checked, because `SelectedFramesOnly` requires every selected frame to
be present in the decoded sequence. This is the narrowest declaration that makes
the truthful answer derivable; the previous no-declaration design bought
"no generator can get it wrong" by making one generator's answer wrong for
everyone.

*Reachability, recorded honestly:* through the MCP surface the filmstrip's plan
is already bounded to `tile_limit` by `bounded_plan`, so decoded == tiles and the
manifest was accidentally correct. The defect was reachable through the
`temporal-vision` crate's public API, which is a real boundary with its own
contract, and its own tests asserted the wrong numbers.

Agreement with the sampling disclosure is enforced rather than coordinated. The
manifest's own `validate()` — which runs on construction *and* on
deserialization — rejects any `analysis_sampling` parameter block whose
`source_frame_count` or `analyzed_frame_count` disagrees with the manifest
counts, and requires the block to exist when an analysis artifact actually
dropped frames. The tool warning reads that block, so warning and manifest
cannot diverge without a manifest that will not construct.

**The whole disclosure is validated, not only its counts.** *Revised after
cross-model review.* Reconciling `source_frame_count` and `analyzed_frame_count`
left a manifest free to agree about *how many* frames were examined while lying
about *which* — a missing or arbitrary `analyzed_source_indices`, an invented
`mode`, an invented `spacing`, or an extra field carrying an unvalidated claim.
Validation now requires: `mode` and `spacing` to name the one scheme these
generators produce; `analyzed_source_indices` to be present, as long as the
analyzed count, strictly increasing, and within the source frames; and the field
set to be exactly the five documented members. The tampering test exercises each
of the eight edits independently — the previous single-edit test missed all but
one of them.

## Design decisions

- **Emission is guarded on real decimation, not on the presence of source
  provenance.** `decode_plan` always attaches provenance, so `source_indices()`
  is always `Some` in production and the old check was vacuous. The guard is now
  `indices.len() < source_frame_count`, matching what the storyboard path
  already did. An exhaustive run emits no `analysis_sampling` block at all, so
  the block's presence is itself the truthful signal that sampling occurred.
- **The disclosure-agreement invariant is limited to the two analysis kinds for
  *required presence*, but applies to *any* kind for *agreement*.** Difference
  map and motion history are exactly the kinds the MCP warning covers, so those
  must disclose when decimated. Region filmstrip carries its selection
  provenance through `artifact_source_indices` / `strip_omitted_frame_count`
  instead and is not forced to emit a second block.
- **Only an explicitly named reference frame is threaded into `bounded_plan`.**
  `FrameSelector::First` and `Last` need no retention because uniform selection
  always keeps both endpoints; passing them would be noise that could displace a
  grid frame for no gain.
- **Error context is carried on `EpochPlan`/`EpochInput` rather than rebuilt at
  each error site.** `EpochPlan::error_context()` derives session, target, and
  session-time range from frame metadata it already holds. Encoded bytes,
  filesystem paths, and cache identities are never included; frame identity
  appears only in messages, where it already appears in resolved ranges.

## Implementation Units

1. `crates/temporal-vision/src/provenance.rs` — `analyzed_frame_ids` /
   `analyzed_frame_count` manifest fields and accessors; `omitted_frame_count`
   re-derived; `validate_selected_subsequence` generalized to
   `validate_ordered_subsequence` and applied twice (analyzed ⊆ source,
   selected ⊆ analyzed); `validate_analysis_sampling_disclosure`;
   decimation guard in `analysis_sampling_parameters`.
2. `src/artifacts/service.rs` — `explicit_reference_frame` threads a
   `FrameSelector::Frame(id)` reference through `plan_for_analysis_sampling`
   into `bounded_plan`'s existing retention mechanism; `.max(1)` floor on the
   `analysis_effective_max_frames` divisor.
3. `src/artifacts/epoch.rs` — `EpochPlan::error_context()`, `EpochInput.context`,
   and `ErrorContext` attached to decode, adaptation, and bounded-selection
   failures.
4. `src/artifacts/generators.rs` — reference-frame failure names the frame,
   carries context and recovery, and no longer claims the frame is outside the
   epoch when sampling was the cause.
5. `scripts/check-wire-enum-schemas.sh` — inline enum bodies are parsed for
   variants instead of falling through the per-line scan; two self-test fixtures
   added.
6. `crates/temporal-vision/src/provenance.rs` — `mask` boxed, following the
   manifest's existing treatment of rarely populated members. The two added
   fields widened `ArtifactManifest` enough to trip
   `clippy::large_enum_variant` on `krometrail_core::ArtifactOutcome`.

## Required follow-up outside this feature's file ownership

These two changes are needed but live in crates this work was not permitted to
touch. The first leaves one workspace test red until applied.

- `crates/krometrail-mcp/src/server.rs:3429` —
  `assert_eq!(compact["omitted_frame_count"], 1);` must become `0`. Its fixture
  is a one-source-frame difference map with no selected frames; under the
  corrected contract that frame was analyzed, so nothing was omitted.
- `crates/krometrail-core/src/artifacts.rs:789` — `ArtifactOutcome::Available`
  should hold `Box<ArtifactHandle>`. Boxing the manifest's mask bought enough
  headroom for now, but a large success payload beside a small error variant is
  the actual shape clippy is objecting to, and the next manifest field will trip
  it again.

## Testing

- `temporal-vision/tests/difference_map.rs` — decimated manifest counts agree
  with the sampling disclosure; an undecimated run with source provenance emits
  no block; a tampered disclosure is rejected on deserialization.
- `src/artifacts/service_tests.rs` — end-to-end: bounded sampling retains an
  off-grid `FrameSelector::Frame` reference; 367→120 sampling reports
  `omitted 247` and matches its disclosure; exhaustive claims no mode; the
  frame-budget divisor floor.
- Existing `omitted_frame_count` assertions in `contracts.rs`,
  `difference_map.rs`, `motion_history.rs`, and `filmstrip.rs` were updated to
  the corrected contract and strengthened with the analyzed/selected
  distinction. No assertion was removed or weakened.
- `scripts/check-wire-enum-schemas.sh` self-tests cover a bare and a renamed
  single-line enum body.

## Risks

- `omitted_frame_count` changes meaning for every artifact kind, not only the
  analysis kinds. This is deliberate — the old value was false in the same way
  everywhere — but it is a wire-visible semantic change to a field agents may
  already read.
- The manifest gains two required wire fields, so retained artifact cache from
  before this change will not deserialize and will be treated as an incompatible
  format. That matches the one-current-format rule.
- `RegionFilmstrip` has a `omitted_frame_count` *parameter* that means
  "analyzed but not rendered" while the manifest field now means "contributed
  nothing". Both are truthful; the name collision is a readability wart worth a
  follow-up rename.
- The MCP compact projection exposes `source_frame_count`,
  `selected_frame_count`, and `omitted_frame_count` but not
  `analyzed_frame_count`, so compact readers cannot derive the
  analyzed-but-unrendered figure without fetching the manifest. Follow-up.

## Acceptance

- Manifest counts and the tool warning agree for every generator, derived from
  one source.
- No manifest claims a sampling mode that was not applied.
- An explicit frame reference survives `uniform_bounded` sampling.
- Regression coverage for each of the three.

## Second cross-model review round: disclosure index correlation

`analyzed_source_indices` was validated for shape only — size, range, strict
ordering, mode, spacing, unknown fields — and never against `analyzed_frame_ids`.
A disclosure could therefore be structurally perfect and still name frames the
analysis never examined: source `[f0,f1,f2,f3]`, analyzed `[f0,f2]`, disclosed as
`[0,1]` passed every check while identifying `f0` and `f1`.

Fixed in `crates/temporal-vision/src/provenance.rs`
(`validate_analysis_sampling_disclosure`). Each index must now resolve, through
the manifest's own `source_frame_ids`, to exactly the corresponding entry of
`analyzed_frame_ids`; a mismatch fails with "analysis sampling source index does
not identify the analyzed frame". The check runs after the range and ordering
checks so their existing diagnostics are unchanged.

Regression: a ninth entry in the tampering table of
`a_manifest_may_not_be_deserialized_with_a_contradictory_sampling_disclosure`
(`crates/temporal-vision/tests/difference_map.rs`) rewrites `[0,3,5,8]` to
`[0,1,5,8]` — right length, in range, strictly increasing, and naming a dropped
frame. Pre-fix the tampered manifest deserialized successfully.
