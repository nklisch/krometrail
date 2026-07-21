---
id: feature-analysis-provenance-truthfulness
kind: feature
stage: drafting
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

## Acceptance

- Manifest counts and the tool warning agree for every generator, derived from
  one source.
- No manifest claims a sampling mode that was not applied.
- An explicit frame reference survives `uniform_bounded` sampling.
- Regression coverage for each of the three.
