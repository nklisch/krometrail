---
id: idea-analysis-sampling-provenance-accuracy
created: 2026-07-20
updated: 2026-07-20
tags: [visual, agent-ux]
---

Two provenance/selection defects in the analysis-generator sampling path, both
found in the pass-3 review of the v1.2.7 work. Neither is silent evidence
corruption, which is why they were parked rather than fixed in-cycle — but the
first is a false provenance claim in a temporal-evidence product, so it is worth
doing properly rather than leaving indefinitely.

**1. Undecimated analysis manifests falsely claim `mode: "uniform_bounded"`.**
`decode_plan` (`src/artifacts/epoch.rs:283-291`) always attaches source
provenance, so `source_indices()` is always `Some` in production. Consequently
`analysis_sampling_parameters` (`crates/temporal-vision/src/provenance.rs:268`)
emits an `analysis_sampling` block with hardcoded `mode: "uniform_bounded"` and
`spacing: "uniform"` on *every* difference map and motion history — including
exhaustive, undecimated runs.

Counts are equal in that case, so the top-level MCP warning is correctly
suppressed and no agent is told its evidence is degraded when it is not. The
defect is narrower: an agent auditing the manifest of an exhaustive run reads a
sampling mode that was neither requested nor applied.

The storyboard path already does this correctly — it guards emission on actual
decimation (`src/artifacts/generators.rs:175-179`). The analysis path should
either guard the same way, or record the *requested* mode rather than a
hardcoded one.

**2. `uniform_bounded` plus an explicit frame reference can drop that frame.**
`plan_for_analysis_sampling` (`src/artifacts/service.rs:690`) passes `None` as
`bounded_plan`'s `include_frame_id`, even though `bounded_plan` has a retention
mechanism for exactly this purpose (the filmstrip locator uses it).

With the wire-default sampling on a large range, a `FrameSelector::Frame(id)`
reference that falls off the uniform grid fails with "reference frame is outside
this visual epoch" (`src/artifacts/generators.rs:685`). That message is
misleading — the frame *is* in the epoch; sampling dropped it — and the failure
is avoidable by threading the reference id through to `bounded_plan`.

Explicit failure rather than silent corruption, but a confusing one.

**Nits worth folding in if this is picked up:**

- `analysis_effective_max_frames` (`src/artifacts/service.rs:695`) divides by the
  per-frame max with no defensive floor. Safe only via the non-empty-plan
  invariant; a `.max(1)` on the divisor would make the panic structurally
  impossible rather than invariant-dependent.
- `scripts/check-wire-enum-schemas.sh` misses single-line enum bodies
  (`enum H { A, B }`). Unreachable in-tree because `cargo fmt --check` is
  enforced and rustfmt always breaks enum bodies, but it is a real gap in the
  guard's logic.

Origin: 2026-07-20 pass-3 cross-model review of the sixth-shakedown work
(v1.2.7). Verdict was SHIP with these as explicit follow-ups.
