---
id: feature-artifact-generator-ergonomics
kind: feature
stage: implementing
tags: [visual, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Artifact generator ergonomics

## Brief

The two exhaustive artifact generators — `difference_map` and `motion_history` —
are effectively undiscoverable. In the 2026-07-20 sixth shakedown it took four
failed calls against a real animated page to get either to produce anything, and
the only reason the ceiling was found at all is that an unrelated filmstrip
manifest happened to leak `max_source_frames: 120`.

Two independent problems, both live-reproduced on threejs.org WebGL keyframes
(744 frames, 44fps, 1673x1288 viewport):

**(a) Errors carry no recovery and no numbers.** `src/artifacts/service.rs`
returns `"exhaustive artifact generator exceeds the source-frame limit"` and
`"artifact generator exceeds the decoded-byte limit"`, both with `recovery: null`
and neither stating the limit or the actual value. The observed sequence:

| Attempt | Frames | Result |
|---|---|---|
| 1 | 744 | source-frame limit (no number) |
| 2 | 101 | decoded-byte limit — a *different* limit, no number |
| 3 | 101 + factor-7 downscale | same byte error; scale does not help |
| 4 | 43 | succeeds |

An agent has no path to that answer except binary-searching the range. Compare
the `fit_limits` failure fixed in v1.2.6, which does carry proper recovery text.

**(b) The exhaustive generators hard-error where the strip generators decimate.**
The machinery to do the right thing already exists and is simply not wired up:

```rust
Storyboard      => bounded_plan(...)   // decimates, discloses
RegionFilmstrip => bounded_plan(...)   // decimates, discloses
DifferenceMap | MotionHistory => hard error
```

The manifest disclosure block is already built too — `analysis_sampling`
(`crates/temporal-vision/src/render.rs:1050`) carries `source_frame_count`,
`analyzed_frame_count`, and `analyzed_source_indices`.

Practical effect today: exhaustive analysis caps at roughly 1–2 seconds of
animation on a standard viewport, and nothing in the surface says so.

## Simplification opportunity

- Removes a bespoke hard-error branch in `service.rs` in favour of the
  `bounded_plan` path the sibling generators already use — one selection
  strategy instead of two.
- Reuses the existing `analysis_sampling` manifest block rather than adding a
  second disclosure mechanism.

## Design intent

Recorded from the agreed design discussion; `feature-design` owns the details.

**Error quality.** Every resource-limit error must state the limit, the actual
value, and the lever that moves it. `recovery: null` on a `retry: "never"` error
is a defect in its own right and must be fixed regardless of what decimation
lands — some requests will still legitimately fail.

**Likely ordering bug.** `generator_plan.decoded_bytes` is evaluated against the
**source** plan before normalization is applied, which is why an explicit
factor-7 downscale did not reduce it. The adjacent accumulator check in
`crates/temporal-vision/src/difference_map.rs:142` correctly uses
`normalized.dimensions()`. Design should establish whether the pre-normalization
evaluation is deliberate (guarding decode cost, which is genuinely paid on the
source) or an ordering mistake — the two checks disagreeing is the signal worth
chasing. If it is deliberate, the error text must say so, because a caller
reasonably expects `normalization.scale` to be the lever.

**Auto-decimation with disclosure.** Wire `DifferenceMap`/`MotionHistory` to
`bounded_plan`. Surface sampling as a **top-level response warning**, not only in
the manifest — the agent should not need a second fetch to learn its evidence was
subsampled. This follows the existing `bounded-loss-accounting` pattern.

**`count` mode carve-out — the crux.** Decimation is not semantically free:

- `motion_history` — decay-weighted accumulation. Sampling shifts the decay curve
  but the artifact stays qualitative. Safe with disclosure.
- `difference_map` / `normalized_frequency` — normalized, survives sampling well.
- `difference_map` / `count` — literally counts changes per pixel. Sampling 43 of
  744 frames does not approximate the answer, it changes it by ~17x. Silently
  returning a decimated count map is quietly wrong, which is worse than today's
  hard error.

For `count` mode the decision is refuse-and-explain rather than return
sample-scaled numbers, on the grounds that a scaled estimate presented as a count
invites misreading. Design may revisit, but must not make it silent.

**Explicit opt-out.** Callers need to keep expressing "analyse every frame, fail
if you cannot" — e.g. `sampling: exhaustive | uniform_bounded`.

## Risks

- Changing exhaustive generators to sample by default alters the meaning of
  existing artifacts. The disclosure is what makes this acceptable; if the
  warning is easy to miss, the change is net-negative.
- The `count`-mode carve-out means one generator behaves differently per mode,
  which is a wart. The alternative — silently wrong counts — is worse, but design
  should confirm there is no third option (e.g. exact counts over a declared
  sampled window).
- Changing the decoded-byte check to post-normalization, if that is the
  resolution, could raise memory ceilings in a way that needs its own bound.

Origin: 2026-07-20 sixth shakedown against v1.2.6.

## Design decisions

- **The pre-normalization `decoded_bytes` check is NOT a bug — the brief was
  wrong.** Design investigation overturns the scope-time hypothesis. `decoded_bytes`
  sums across *source* frames (`src/artifacts/epoch.rs:167`) while the accumulator
  check uses `normalized.dimensions()`
  (`crates/temporal-vision/src/difference_map.rs:142`). These are two different
  resources, each measured correctly: a source PNG must be decoded at full size
  *before* it can be downscaled, so decode cost is genuinely paid on the source,
  whereas the accumulator is genuinely allocated after normalization. The two
  checks disagreeing is correct behavior, not a mistake.

  This makes the error-message defect worse, not better: the shakedown agent
  concluded from the message that `normalization.scale` was the lever, and spent
  two of its four failed calls acting on that wrong inference. **A caller cannot
  be expected to derive this from "exceeds the decoded-byte limit."** The error
  must state which resource is exhausted and that frame count — not scale — is
  the lever for this one.

- **Auto-decimation is the primary fix; better errors are the backstop.** Once
  the exhaustive generators decimate, the frame-count limit stops being reachable
  by normal use. The error path still has to be correct for the cases decimation
  cannot rescue (`count` mode, explicit `exhaustive` opt-out, single frames too
  large to decode at all).

- **`count` mode refuses rather than returns scaled estimates.** Confirmed from
  the brief. A count map is an exact measurement; sampling 43 of 744 frames
  changes it ~17x. Returning that silently is quietly wrong, and returning it
  scaled invites reading an estimate as a measurement.

- **Open for implementation to establish: is the aggregate decode bound
  necessary?** `decoded_bytes` is a `try_fold` sum across all frames, implying
  every frame is held decoded simultaneously. `difference_map` and
  `motion_history` both *accumulate* — they may not need more than a reference
  frame plus the current one resident. If decode is or can be streamed, the
  aggregate bound is far more conservative than the real constraint and the
  practical ceiling rises substantially. Investigate, but do not let it block the
  feature: decimation plus honest errors is the deliverable, streaming decode is
  an optimization that can be parked if it is not shallow.

## Architectural choice

**Chosen: route the exhaustive generators through the existing `bounded_plan`
path and report sampling as first-class evidence quality.**

`bounded_plan(plan, max_frames, include_frame_id)` (`src/artifacts/epoch.rs:115`)
already does evenly-spaced selection, already preserves temporal endpoints, and
was hardened in v1.2.6 for exactly the gap- and marker-correctness problems that
naive subsampling causes. Storyboard and RegionFilmstrip already use it. Reusing
it means the exhaustive generators inherit that correctness rather than growing a
second, untested selection strategy.

Considered and rejected:

- *Raise the limits.* Does not scale — a longer range always re-breaks it, and it
  trades a clear error for an OOM.
- *A separate sampler for exhaustive generators.* Duplicates selection logic that
  took three review passes to get right in v1.2.6.

## Implementation Units

### Unit 1: Resource-accurate limit errors
**File**: `src/artifacts/service.rs`, `src/artifacts/decode.rs`

Every resource-limit error states the limit, the actual value, and the lever.

**Implementation Notes**:
- `limit_error` (`src/artifacts/decode.rs:165`) takes only a message; it needs to
  carry structured limit/actual/lever and populate `recovery` rather than leaving
  it `null`.
- The two messages must be *distinguishable in kind*: the frame-count limit's
  lever is a narrower range or `uniform_bounded` sampling; the decoded-byte
  limit's lever is a narrower range **only** — say so explicitly, since scale is
  the intuitive-but-wrong guess.
- `retry: "never"` is correct for both and should stay; these are deterministic.

**Acceptance Criteria**:
- [ ] No resource-limit error returns `recovery: null`.
- [ ] Each states the configured limit and the observed value.
- [ ] The decoded-byte error explicitly states that `normalization.scale` does
      not reduce it and why.
- [ ] A test asserts the recovery text names a lever that actually works.

### Unit 2: Wire exhaustive generators to bounded selection
**File**: `src/artifacts/service.rs`

Replace the hard-error branch for `DifferenceMap` / `MotionHistory` with
`bounded_plan`, gated on the new sampling mode.

```rust
// replaces the current `if plan.frames.len() > limits.max_source_frames { Err }`
DifferenceMap(request) | MotionHistory(request) => match request.sampling {
    Sampling::Exhaustive    => exact_or_limit_error(plan, limits),
    Sampling::UniformBounded => bounded_plan(plan, limits.max_source_frames.get(), None),
}
```

**Implementation Notes**:
- `include_frame_id` is `None` — unlike a filmstrip there is no locator to pin.
- Default is `uniform_bounded`. This changes existing behavior from "error" to
  "sampled result", which is only acceptable because Unit 3 makes it loud.
- The decoded-byte check still applies *after* selection, and now sees the
  reduced set — so decimation raises the effective ceiling for both limits.

**Acceptance Criteria**:
- [ ] A 744-frame range produces both artifacts by default.
- [ ] `sampling: exhaustive` on the same range still errors, with Unit 1's text.
- [ ] Selected frames are evenly spaced and retain first and last.

### Unit 3: Sampling disclosed at the response top level
**File**: `crates/krometrail-mcp/src/response.rs`, `crates/temporal-vision/src/render.rs`

`analysis_sampling` already exists in the manifest
(`crates/temporal-vision/src/render.rs:1050`). Surface the same fact as a
response-level warning so it cannot be missed without a second fetch.

**Implementation Notes**:
- Follows `bounded-loss-accounting`: sampled analysis is degraded evidence
  quality and belongs in the same channel as capture gaps.
- Must state analyzed-of-total and that spacing is uniform. Do not bury the
  ratio only in `analyzed_source_indices`.

**Acceptance Criteria**:
- [ ] A sampled result carries a top-level warning naming counts and mode.
- [ ] An unsampled result carries no such warning (no false alarms).
- [ ] The manifest `analysis_sampling` block still agrees with the warning.

### Unit 4: `count`-mode carve-out
**File**: `src/artifacts/service.rs`

`difference_map` with `frequency_mode: count` must not silently decimate.

**Implementation Notes**:
- Refuse with an error naming the conflict and offering the two real options:
  narrow the range, or switch to `normalized_frequency`.
- `magnitude` needs a decision: design's reading is that it survives sampling
  (it is an extremum/aggregate, not a tally) so it may decimate — implementation
  should confirm against the accumulator semantics before treating it as safe,
  and fall back to the `count` treatment if uncertain.

**Acceptance Criteria**:
- [ ] `count` + oversized range refuses, naming both options.
- [ ] `count` within limits still succeeds unchanged.
- [ ] `normalized_frequency` decimates and discloses.
- [ ] `magnitude`'s treatment is decided, tested, and its rationale recorded.

## Implementation Order

1. Unit 1 (errors — independent, immediately useful)
2. Unit 2 (bounded selection)
3. Unit 4 (count carve-out — must land with Unit 2, never after)
4. Unit 3 (disclosure — needs Unit 2's sampling signal)

No child stories. Units 2 and 4 are a single semantic change that must not ship
apart: Unit 2 without Unit 4 silently returns wrong count maps. Splitting into
stories would create a checkpoint where the tree is shippable-looking but
semantically broken.

## Testing

- **Regression, live-derived**: the 744-frame / 1673x1288 case from the shakedown
  must produce both artifacts by default and must have refused before this
  feature. This is the test that proves the reported friction is gone.
- **Interface**: `sampling: exhaustive` opt-out still errors; error carries a
  working lever.
- **Correctness of the carve-out**: `count` refuses; `normalized_frequency`
  decimates. This protects against the silent-wrong-answer failure mode, which is
  the one genuinely dangerous outcome here.
- **No-false-alarm**: unsampled results carry no sampling warning — guards
  against warning fatigue making the disclosure worthless.
- Reuse existing `bounded_plan` coverage; do not duplicate selection tests.

## Risks

- **Riskiest: default-to-sampling changes what existing artifacts mean.** The
  disclosure is the entire mitigation. If the top-level warning is easy to
  overlook, this feature is net-negative — an agent that silently analyses 6% of
  frames and reports confidently is worse than one that errors. Unit 3 is not
  optional polish.
- `magnitude` mode is assumed sampling-safe on reasoning, not verified. If that
  assumption is wrong and it ships decimating, it produces quietly wrong output
  in the same way `count` would. Verify before trusting.
- Streaming decode (if pursued) touches memory behavior under concurrency and
  should not be folded in opportunistically.
- `bounded_plan` was hardened in v1.2.6 for gap and marker correctness. New
  callers must not bypass the marker/gap re-clamping that hardening added.
