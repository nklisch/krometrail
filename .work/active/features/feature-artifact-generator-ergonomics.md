---
id: feature-artifact-generator-ergonomics
kind: feature
stage: drafting
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
