---
id: feature-temporal-artifact-generation-robustness
kind: feature
stage: implementing
tags: [bug, visual, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Temporal artifact generation robustness

## Brief

The fifth shakedown (2026-07-19, v1.2.5 live) exercised the temporal artifact
surface against a continuously animating real page (threejs.org WebGL keyframes
example, ~45 fps capture, 1673x1288 viewport). Three defects make the heaviest
and most valuable generators unreliable or unusable exactly when they matter
most — during sustained motion with a saturated capture queue.

1. **`generate_region_filmstrip` hard-fails on ranges whose gaps fall outside
   the selected tile span** with `frame sequence adaptation failed: gap range
   lies outside the frame range`. The identical `range_handle` succeeded through
   `temporal_debug_bundle` / storyboard.
2. **`fit_limits` analysis scale cannot recover on most real viewports.**
   `difference_map` and `motion_history` both failed at defaults with
   `normalized analysis: 42 frames require 543015648 bytes exceeds limit
   536870912 bytes` — over budget by 1.1%, capping usable capture at ~0.9 s.
3. **Diagnostics point at knobs that do not exist or do not say what is legal.**
   The recovery text offers "use a larger artifact budget" (no such request
   field), and the downscale error does not name any admissible factor.

Minor, same area: `motion_history` labels truncate to `MOTION HIS... SEE
MANIFEST` when the artifact renders narrow (239 px at factor 7).

## Root cause

### Defect 2 (fully root-caused)

`fit_scale` (`src/artifacts/generators.rs:354`) searches only
`[1_u8, 2, 4, 8]`. `normalized_dimensions`
(`src/artifacts/generators.rs:466-478`) accepts `AnalysisScale::Down(factor)`
only when `factor` divides **both** width and height exactly, else it errors
`analysis downscale must exactly divide both dimensions`.

For 1673x1288: 1673 = 7 x 239, 1288 = 2^3 x 7 x 23, so gcd = 7. No power of two
divides 1673. Every candidate above Identity is rejected on divisibility, so
once Identity exceeds the budget `fit_scale` exhausts its list and fails, even
though factor 7 is admissible and comfortably fits (543015648 / 49 = 11082972
bytes). Confirmed live: an explicit `{"scale":"down","factor":7}` succeeded.

The exact-division invariant is itself correct — an integer box filter avoids
resampling artifacts that would corrupt pixel measurement. The defect is the
candidate set, not the invariant.

### Defect 1 (reproduced; root cause to confirm)

Reproduction data, captured live:
- Resolved range `[116000000000, 120300000000]`, 184 frames.
- First frame `116013596177`, last frame `120262122018`.
- Gaps: `[119325329378, 119325329378]`, `[120159653105, 120159653105]`, and
  `[120284683060, 120350681771]` (entirely after the last frame; its end also
  exceeds the resolved range end).
- `tile_limit` 3 and 6 both failed; effective artifact anchor was
  `118150000000`.
- The narrower handle `[119200000000, 120200000000]` (42 frames, first
  `119208142216`, last `120195752578`, both gaps interior) succeeded at
  `tile_limit` 6.

`clipped_gaps` (`src/artifacts/epoch.rs:377`) already clips to the epoch frame
span and drops a gap when `clipped_start > clipped_end`, which by hand-check
does drop the trailing gap. So the surviving hypothesis is that the failure is
raised later, against a **narrower span than the epoch frame span** —
`ArtifactManifest`'s own `validate_gaps` (`crates/temporal-vision/src/provenance.rs:618`
against `self.range`) applied to the region filmstrip's **selected tile span**
rather than the source frame span. A gap interior to the frame set but outside
the chosen tiles would then hard-fail.

**Confirm before fixing.** Reproduce as a deterministic unit test using the
numbers above; do not fix on the hypothesis alone. If the manifest range is the
culprit, the fix is to validate gaps against the source frame span (or clip
gaps to the selected span, retaining them as provenance) rather than to relax
`validate_gaps`, which is a sound invariant for the sequence it guards.

Note also that the resolver emitted a gap whose end (`120350681771`) exceeds
the resolved range end (`120300000000`). Gaps should be clipped to the resolved
range at resolution time; an unclipped gap is a latent trap for every consumer.

## Design decisions

- **Widen the fit search to real divisors, keep exact division.** Replace the
  hardcoded `[1,2,4,8]` with candidates derived from the actual normalized
  dimensions: ascending divisors of `gcd(width, height)`, bounded by the
  existing `AnalysisScale::Down` u8 domain and by a sane cap. Return the
  smallest admissible factor that fits the budget, preserving current
  "prefer the least downscaling" behavior. This fixes the general case rather
  than special-casing one viewport.
- **Make the downscale error name legal factors.** When
  `normalized_dimensions` rejects a factor, state the offending dimension(s)
  and list admissible divisors of `gcd(width, height)`, mirroring the
  v1.2.5 canvas-limit diagnostic that already names its knob.
- **Drop the unactionable recovery clause.** `max_normalized_bytes` /
  `max_combined_request_bytes` are deployment limits, not request fields, so
  "use a larger artifact budget" misdirects the caller. Replace with the
  actions a caller can actually take: narrow the range, crop the analysis
  region, or pass an explicit smaller `normalization.scale`.
- **Fix defect 1 at the correct layer, not by weakening the invariant.**
  Gaps are evidence-quality state (bounded-loss-accounting); dropping or
  clipping them must stay explicit, and a gap that cannot be represented in the
  artifact's own span must not silently vanish from the manifest.
- **Suppress rather than truncate labels below a legibility threshold.** A
  truncated `MOTION HIS...` label carries no information and costs pixels; the
  manifest already holds the full provenance the label abbreviates.

## Implementation Units

### Unit 1: Admissible-divisor fit scale
**File**: `src/artifacts/generators.rs`

Replace the `[1_u8, 2, 4, 8]` loop in `fit_scale` with ascending divisors of
`gcd(width, height)` computed from the crop-adjusted dimensions that
`normalized_dimensions` would use. Keep returning `AnalysisScale::Identity` for
factor 1. Keep the existing terminal error path when no divisor fits.

**Acceptance Criteria**:
- [ ] A 1673x1288 epoch of 42 frames that exceeds the budget at Identity
      resolves via `fit_limits` to `Down(7)` and generates successfully.
- [ ] A 1920x1080 epoch still prefers the smallest admissible factor (regression
      that power-of-two-friendly viewports are unchanged).
- [ ] When no divisor fits the budget, the terminal `resource_limit_exceeded`
      error is still returned (no panic, no infinite search).
- [ ] Factor search is bounded by the `u8` domain of `AnalysisScale::Down`.

### Unit 2: Actionable analysis diagnostics
**File**: `src/artifacts/generators.rs`

Give the `analysis downscale must exactly divide both dimensions` error the
offending dimensions and the admissible divisor list. Replace the
"use a larger artifact budget" recovery clause on the normalized-analysis limit
error with caller-actionable guidance.

**Acceptance Criteria**:
- [ ] Explicit `Down(2)` against 1673x1288 reports the offending dimension and
      names the admissible factors.
- [ ] The `resource_limit_exceeded` recovery no longer references a budget the
      caller cannot set; code stays `ResourceLimitExceeded`.

### Unit 3: Region filmstrip gap handling
**Files**: `crates/temporal-vision/src/provenance.rs`,
`crates/temporal-vision/src/filmstrip.rs`, `src/artifacts/epoch.rs`

First reproduce defect 1 deterministically with the recorded numbers, then fix
at whichever layer the reproduction indicts. Do not relax
`sequence::validate_gaps`.

**Acceptance Criteria**:
- [ ] A regression test reproduces `gap range lies outside the frame range`
      from a region filmstrip over a frame set whose gaps sit outside the
      selected tile span, and passes after the fix.
- [ ] The filmstrip generates successfully for that range, and its manifest
      still accounts for every gap in the resolved range (none silently lost).
- [ ] Storyboard, difference-map, and motion-history paths over the same range
      are unchanged.

### Unit 4: Resolver gap clipping
**File**: `crates/krometrail-core/` range resolution (locate the gap assembly
that populates `ResolvedRange::gaps`)

Clip emitted gaps to the resolved range so no gap extends past
`resolved_range.end` or before `resolved_range.start`. Drop a gap that clips
empty only if it carries no missing-frame estimate; otherwise retain the
clipped remainder so loss accounting stays honest.

**Acceptance Criteria**:
- [ ] A resolved range never reports a gap whose start or end lies outside
      `resolved_range`.
- [ ] Known-missing-frame totals are not silently reduced by clipping; if a gap
      is dropped, its estimate is still reflected in the gap summary.

### Unit 5: Label legibility threshold
**File**: `crates/temporal-vision/src/render/` (motion-history / storyboard
label rendering)

Below a minimum legible width, omit the label band rather than rendering
truncated text.

**Acceptance Criteria**:
- [ ] A 239 px-wide motion-history artifact renders without truncated
      `...`-terminated labels.
- [ ] Wide artifacts render labels exactly as today.

## Implementation Order
1. Unit 1 (unblocks the most-cited friction)
2. Unit 2 (same file, same tests)
3. Unit 3 (independent; reproduce first)
4. Unit 4 (independent)
5. Unit 5 (independent, cosmetic)

## Testing
- Unit tests for the divisor search across viewports with distinct gcds
  (1673x1288 -> 7, 1920x1080 -> power-of-two friendly, a prime-width case where
  only Identity is admissible and the terminal error must fire).
- A deterministic regression test for the filmstrip gap failure built from the
  recorded reproduction numbers.
- A resolver test asserting emitted gaps are clipped to the resolved range.
- No new real-Chrome tests; all of this is pure adaptation, layout, and
  validation math reachable from deterministic doubles.

## Risks
- The filmstrip root cause is a hypothesis, not yet confirmed. If the
  reproduction indicts a different layer, re-derive before changing code; a
  wrong fix here would weaken gap accounting, which is load-bearing evidence
  state.
- Unit 4 touches loss accounting. Clipping must not reduce reported missing
  frames, or the honesty guarantee in `bounded-loss-accounting` regresses.
- Widening the divisor search changes which scale `fit_limits` picks on some
  viewports, so cached artifacts keyed by resolved scale may miss once. That is
  acceptable; the cache key already includes the resolved scale.

Origin: 2026-07-19 fifth shakedown friction report.
